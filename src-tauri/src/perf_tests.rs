//! Large-corpus timings. Ignored by default; run with
//! `cargo test --release --lib perf -- --ignored --nocapture`.

use std::{collections::BTreeMap, time::Instant};

use rusqlite::params;
use tempfile::tempdir;
use uuid::Uuid;

use crate::{
    db::{Database, now},
    export::ExportRequest,
    models::CreateProjectInput,
    screening::DecisionInput,
};

const ARTICLES: usize = 1_500;

fn timed<T>(label: &str, budget_ms: u128, task: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let value = task();
    let elapsed = start.elapsed().as_millis();
    println!("{label:<42} {elapsed:>6} ms (objetivo < {budget_ms} ms)");
    assert!(elapsed < budget_ms, "{label} tardó {elapsed} ms");
    value
}

#[test]
#[ignore = "benchmark; run explicitly with --ignored"]
fn large_corpus_stays_interactive() {
    let directory = tempdir().unwrap();
    let database = Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
    database
        .open()
        .unwrap()
        .execute(
            "UPDATE app_settings SET researcher_name = 'Ana' WHERE id = 1",
            [],
        )
        .unwrap();
    let project = database
        .create_project(CreateProjectInput {
            name: "Corpus grande".into(),
            description: String::new(),
            review_type: "systematic_mapping".into(),
        })
        .unwrap()
        .id;

    let ids: Vec<String> = timed("insertar artículos", 5_000, || {
        let mut connection = database.open().unwrap();
        let transaction = connection.transaction().unwrap();
        let ids = (0..ARTICLES)
            .map(|index| {
                let id = Uuid::new_v4().to_string();
                transaction
                    .execute(
                        "INSERT INTO articles (id, title, abstract_text, publication_year, doi, authors_json, keywords_json, metadata_source, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, '[\"Autor, Uno\"]', '[\"heritage\"]', 'ris', ?6, ?6)",
                        params![
                            id,
                            format!("Study {index} on heritage segmentation and learning"),
                            format!("Abstract {index}: murals, photographs and continual learning."),
                            2000 + (index % 26) as i64,
                            format!("10.1000/bench.{index}"),
                            now()
                        ],
                    )
                    .unwrap();
                id
            })
            .collect();
        transaction.commit().unwrap();
        ids
    });
    timed("añadir al corpus (Id secuencial)", 5_000, || {
        database
            .add_to_corpus(&project, &ids, Some("Scopus"))
            .unwrap()
    });
    timed("1500 decisiones de cribado", 15_000, || {
        for (index, id) in ids.iter().enumerate() {
            database
                .record_screening_decision(DecisionInput {
                    project_id: project.clone(),
                    article_id: id.clone(),
                    phase: "metadata".into(),
                    decision: if index % 3 == 0 { "exclude" } else { "include" }.into(),
                    criterion_id: None,
                    criteria: BTreeMap::new(),
                    reason: "fuera de alcance".into(),
                })
                .unwrap();
        }
    });
    let table = timed("tabla de cribado", 2_000, || {
        database.screening_table(&project, "metadata").unwrap()
    });
    assert_eq!(table.rows.len(), ARTICLES);
    timed("búsqueda en biblioteca (FTS)", 1_000, || {
        database.library_articles(Some("continual")).unwrap()
    });
    timed("conteos PRISMA", 1_000, || {
        database.prisma_counts(&project).unwrap()
    });
    timed("matriz de extracción", 2_000, || {
        database.extraction_matrix(&project, true).unwrap()
    });
    timed("síntesis descriptiva", 3_000, || {
        database.synthesis(&project).unwrap()
    });
    timed("exportar Excel ampliado", 10_000, || {
        database
            .export_project(ExportRequest {
                project_id: project.clone(),
                format: "xlsx".into(),
                variant: Some("extended".into()),
                scope: None,
                path: directory
                    .path()
                    .join("corpus.xlsx")
                    .to_string_lossy()
                    .into(),
            })
            .unwrap()
    });
    timed("respaldo completo", 10_000, || {
        database
            .create_backup(&directory.path().join("respaldo.zip"))
            .unwrap()
    });
    let package = directory.path().join("intercambio.json");
    timed("exportar paquete de intercambio", 5_000, || {
        database
            .export_exchange_package(&project, &package)
            .unwrap()
    });
}
