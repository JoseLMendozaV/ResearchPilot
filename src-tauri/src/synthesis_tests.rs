use std::collections::BTreeMap;

use tempfile::{TempDir, tempdir};

use super::*;
use crate::{
    matrix::CellInput,
    models::CreateProjectInput,
    review::{FieldInput, QuestionInput, StrategyInput},
    screening::{CriterionInput, DecisionInput},
};

fn record(title: &str, doi: &str, year: i64) -> String {
    format!(
        "TY  - JOUR\nTI  - {title}\nAU  - Author, {title}\nPY  - {year}\nDO  - {doi}\nER  - \n\n"
    )
}

struct Fixture {
    directory: TempDir,
    database: Database,
    project_id: String,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempdir().unwrap();
        let database =
            Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
        let project_id = database
            .create_project(CreateProjectInput {
                name: "Murales & frescos".into(),
                description: String::new(),
                review_type: "systematic_review".into(),
            })
            .unwrap()
            .id;
        Self {
            directory,
            database,
            project_id,
        }
    }

    fn import(&self, database_name: &str, content: &str) {
        let strategies = self
            .database
            .save_search_strategy(StrategyInput {
                id: None,
                project_id: self.project_id.clone(),
                database_name: database_name.into(),
                query_text: "mural".into(),
                query_date: "2026-10-01".into(),
                result_count: 0,
                notes: String::new(),
                filters: json!({}),
                language: Some("en".into()),
                period_start: None,
                period_end: None,
                new_version: false,
            })
            .unwrap();
        let strategy = strategies
            .iter()
            .find(|strategy| strategy.database_name == database_name)
            .unwrap();
        let path = self.directory.path().join(format!("{database_name}.ris"));
        std::fs::write(&path, content).unwrap();
        self.database
            .import_strategy_results(&strategy.id, &path.to_string_lossy())
            .unwrap();
    }

    fn article(&self, doi: &str) -> String {
        self.database
            .open()
            .unwrap()
            .query_row("SELECT id FROM articles WHERE doi = ?1", [doi], |row| {
                row.get(0)
            })
            .unwrap()
    }

    fn decide(&self, doi: &str, phase: &str, decision: &str, criterion: Option<&str>) {
        self.database
            .record_screening_decision(DecisionInput {
                project_id: self.project_id.clone(),
                article_id: self.article(doi),
                phase: phase.into(),
                decision: decision.into(),
                criterion_id: criterion.map(str::to_owned),
                criteria: BTreeMap::new(),
                reason: String::new(),
            })
            .unwrap();
    }
}

/// Scopus: m1 h2 p3 · IEEE: m1 (duplicate) n4 · library: x5 · p3 removed.
fn scenario() -> (Fixture, String) {
    let fixture = Fixture::new();
    fixture.import(
        "Scopus",
        &(record("Mural one", "10.1/m1", 2024)
            + &record("Heritage two", "10.1/h2", 2023)
            + &record("Photo three", "10.1/p3", 2022)),
    );
    fixture.import(
        "IEEE Xplore",
        &(record("Mural one", "10.1/m1", 2024) + &record("Network four", "10.1/n4", 2021)),
    );
    let library = fixture.directory.path().join("library.ris");
    std::fs::write(&library, record("Extra five", "10.1/x5", 2020)).unwrap();
    fixture
        .database
        .import_reference_files(vec![library.to_string_lossy().into_owned()])
        .unwrap();
    fixture
        .database
        .add_to_corpus(
            &fixture.project_id,
            &[fixture.article("10.1/x5")],
            Some("Biblioteca"),
        )
        .unwrap();
    fixture
        .database
        .set_corpus_flags(
            &fixture.project_id,
            &fixture.article("10.1/p3"),
            None,
            Some(true),
            None,
        )
        .unwrap();
    let criteria = fixture
        .database
        .save_criterion(CriterionInput {
            id: None,
            project_id: fixture.project_id.clone(),
            criterion_type: "exclusion".into(),
            code: "E1".into(),
            name: "Fuera de tema".into(),
            description: String::new(),
        })
        .unwrap();
    (fixture, criteria[0].id.clone())
}

#[test]
fn prisma_counts_come_from_recorded_events_and_balance() {
    let (fixture, off_topic) = scenario();
    fixture.decide("10.1/m1", "metadata", "include", None);
    fixture.decide("10.1/h2", "metadata", "awaiting_full_text", None);
    fixture.decide("10.1/n4", "metadata", "exclude", Some(&off_topic));
    fixture.decide("10.1/m1", "full_text", "include", None);

    let counts = fixture.database.prisma_counts(&fixture.project_id).unwrap();
    assert_eq!(
        counts.databases,
        vec![
            SourceCount {
                source: "IEEE Xplore".into(),
                records: 2
            },
            SourceCount {
                source: "Scopus".into(),
                records: 3
            }
        ]
    );
    assert_eq!(
        (
            counts.records_from_databases,
            counts.records_from_other_sources,
            counts.duplicates_removed,
            counts.removed_other_reasons
        ),
        (5, 1, 1, 1)
    );
    assert_eq!(
        (
            counts.records_screened,
            counts.records_excluded,
            counts.screening_pending,
            counts.reports_sought
        ),
        (4, 1, 1, 2)
    );
    assert_eq!(
        (
            counts.reports_not_retrieved,
            counts.reports_assessed,
            counts.full_text_pending,
            counts.studies_included
        ),
        (1, 1, 0, 1),
        "h2 has no PDF and no full-text decision"
    );
    assert!(counts.issues.is_empty(), "{:?}", counts.issues);
    assert!(!counts.complete, "x5 still waits for screening");

    fixture.decide("10.1/h2", "full_text", "exclude", Some(&off_topic));
    fixture.decide("10.1/x5", "metadata", "exclude", Some(&off_topic));
    let counts = fixture.database.prisma_counts(&fixture.project_id).unwrap();
    assert_eq!(
        (
            counts.reports_not_retrieved,
            counts.reports_assessed,
            counts.reports_excluded
        ),
        (0, 2, 1)
    );
    assert_eq!(
        counts.exclusion_reasons,
        vec![ReasonCount {
            reason: "Fuera de tema".into(),
            reports: 1
        }]
    );
    assert!(counts.complete && counts.issues.is_empty());

    let mut broken = counts.clone();
    broken.records_screened += 1;
    broken.exclusion_reasons.clear();
    let issues = check_flow(&broken);
    assert!(
        issues.iter().any(|issue| issue.contains("cribados"))
            && issues.iter().any(|issue| issue.contains("motivos"))
    );

    let svg = prisma_svg(&counts, "Murales & frescos");
    assert!(
        svg.starts_with("<svg")
            && svg.contains("Murales &amp; frescos")
            && !svg.contains("Murales & frescos")
    );
    assert!(
        svg.contains("Scopus (n = 3)")
            && svg.contains("Duplicados (n = 1)")
            && svg.contains("• Fuera de tema (n = 1)")
            && svg.contains("(n = 1)</text>")
    );
    assert!(
        !svg.contains("provisional"),
        "complete flows are not marked provisional"
    );
}

#[test]
fn incomplete_flows_are_marked_provisional_in_the_diagram() {
    let (fixture, _) = scenario();
    let counts = fixture.database.prisma_counts(&fixture.project_id).unwrap();
    assert_eq!(counts.screening_pending, 4);
    assert!(prisma_svg(&counts, "X").contains("Diagrama provisional"));
}

#[test]
fn synthesis_uses_confirmed_values_and_drafts_trace_every_claim() {
    let (fixture, off_topic) = scenario();
    for doi in ["10.1/m1", "10.1/h2"] {
        fixture.decide(doi, "metadata", "include", None);
        fixture.decide(doi, "full_text", "include", None);
    }
    fixture.decide("10.1/n4", "metadata", "exclude", Some(&off_topic));
    let project = fixture.project_id.clone();
    let overview = fixture
        .database
        .save_research_question(QuestionInput {
            id: None,
            project_id: project.clone(),
            parent_id: None,
            code: None,
            question: "¿Qué enfoque se usa?".into(),
            objective: String::new(),
            category: String::new(),
        })
        .unwrap();
    let rq = overview.questions[0].id.clone();
    let field =
        |name: &str, data_type: &str, taxonomy: Vec<String>, unit: Option<&str>| FieldInput {
            id: None,
            project_id: project.clone(),
            research_question_id: Some(rq.clone()),
            name: name.into(),
            description: String::new(),
            data_type: data_type.into(),
            unit: unit.map(str::to_owned),
            taxonomy,
            instructions: String::new(),
            required: false,
            examples: Vec::new(),
            master_field_key: None,
        };
    fixture
        .database
        .save_extraction_field(field(
            "Enfoque",
            "category",
            vec!["Supervisado".into(), "Semisupervisado".into()],
            None,
        ))
        .unwrap();
    let overview = fixture
        .database
        .save_extraction_field(field("Exactitud", "number", Vec::new(), Some("%")))
        .unwrap();
    let (approach, accuracy) = (
        overview.questions[0].fields[0].id.clone(),
        overview.questions[0].fields[1].id.clone(),
    );
    let cell = |doi: &str, field: &str, value: &str, status: &str| {
        fixture
            .database
            .save_extraction_value(CellInput {
                project_id: project.clone(),
                article_id: fixture.article(doi),
                field_id: field.into(),
                value_id: None,
                value_text: Some(value.into()),
                status: status.into(),
                notes: String::new(),
                evidence_id: None,
                quote: None,
                page: None,
            })
            .unwrap();
    };
    cell("10.1/m1", &approach, "Semisupervisado", "confirmed");
    cell("10.1/h2", &approach, "Semisupervisado", "confirmed");
    cell("10.1/h2", &approach, "Supervisado", "proposed");
    cell("10.1/m1", &accuracy, "90", "confirmed");
    cell("10.1/h2", &accuracy, "80", "confirmed");

    let synthesis = fixture.database.synthesis(&project).unwrap();
    assert_eq!(synthesis.included, 2);
    assert_eq!(
        synthesis
            .by_year
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        vec!["2023", "2024"]
    );
    let approach_summary = synthesis
        .fields
        .iter()
        .find(|item| item.name == "Enfoque")
        .unwrap();
    assert_eq!(
        approach_summary.frequencies.len(),
        1,
        "the proposed value is not counted"
    );
    assert_eq!(
        (
            approach_summary.frequencies[0].count,
            approach_summary.unverified_values
        ),
        (2, 1)
    );
    let accuracy_summary = synthesis
        .fields
        .iter()
        .find(|item| item.name == "Exactitud")
        .unwrap();
    let numbers = &accuracy_summary.numeric[0];
    assert_eq!(
        (
            numbers.unit.as_str(),
            numbers.n,
            numbers.min,
            numbers.max,
            numbers.median
        ),
        ("%", 2, 80.0, 90.0, 85.0)
    );
    assert!(
        accuracy_summary
            .warnings
            .iter()
            .any(|warning| warning.contains("comparables"))
    );

    let drafts = fixture
        .database
        .generate_draft(&project, "results")
        .unwrap();
    let results = &drafts[0].content;
    assert!(results.contains("se incluyeron 2 estudios"), "{results}");
    assert!(
        results.contains("Semisupervisado en 2 estudio(s) [Id 1, 2]"),
        "{results}"
    );
    assert!(results.contains("[1 valor(es) sin confirmar en «Enfoque» no se consideraron.]"));
    assert!(results.contains("[Provisional"), "x5 is still pending");
    let drafts = fixture
        .database
        .save_draft(&project, "results", "Texto revisado por la investigadora.")
        .unwrap();
    assert!(drafts[0].edited);
    let methods = fixture
        .database
        .generate_draft(&project, "methods")
        .unwrap()
        .into_iter()
        .find(|draft| draft.section == "methods")
        .unwrap();
    assert!(
        methods
            .content
            .contains("Criterios de exclusión: Fuera de tema")
            && methods
                .content
                .contains("[Pendiente: el protocolo aún no está aprobado.]")
    );
    let limitations = fixture
        .database
        .generate_draft(&project, "limitations")
        .unwrap()
        .into_iter()
        .find(|draft| draft.section == "limitations")
        .unwrap();
    assert!(
        limitations.content.contains("un único revisor") && limitations.content.contains("en"),
        "{}",
        limitations.content
    );
    assert!(
        fixture
            .database
            .generate_draft(&project, "discussion")
            .is_err()
    );
    let edits = fixture
        .database
        .project_audit(&project, 100)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.entity_type == "synthesis_draft")
        .count();
    assert_eq!(edits, 4);
}
