use std::collections::BTreeMap;

use calamine::{Data, Reader, Xlsx, open_workbook};
use tempfile::{TempDir, tempdir};

use super::*;
use crate::{
    matrix::CellInput,
    models::CreateProjectInput,
    quality::{QualityCriterionInput, ScaleOption, ScoreEntry},
    review::{FieldInput, QuestionInput, StrategyInput},
    screening::{CriterionInput, DecisionInput},
};

const RIS: &str = "TY  - JOUR\nTI  - Semi-supervised segmentation of murals & frescoes\nAU  - Researcher, Ana\nPY  - 2024\nDO  - 10.1000/mural.1\nJO  - Heritage Science\nKW  - segmentation\nAB  - We reach 95% accuracy_overall.\nER  - \n\nTY  - JOUR\nTI  - Continual learning for heritage images\nAU  - Scientist, Luis\nPY  - 2023\nDO  - 10.1000/heritage.2\nER  - \n";

struct Fixture {
    directory: TempDir,
    database: Database,
    project_id: String,
}

fn fixture() -> Fixture {
    let directory = tempdir().unwrap();
    let database = Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
    let project_id = database
        .create_project(CreateProjectInput {
            name: "Mapeo de patrimonio".into(),
            description: String::new(),
            review_type: "systematic_mapping".into(),
        })
        .unwrap()
        .id;
    let fixture = Fixture {
        directory,
        database,
        project_id,
    };
    fixture.populate();
    fixture
}

impl Fixture {
    fn populate(&self) {
        let project = self.project_id.clone();
        let database = &self.database;
        let strategies = database
            .save_search_strategy(StrategyInput {
                id: None,
                project_id: project.clone(),
                database_name: "Scopus".into(),
                query_text: "heritage AND (segmentation OR \"continual learning\")".into(),
                query_date: "2026-10-01".into(),
                result_count: 2,
                notes: String::new(),
                filters: json!({}),
                language: None,
                period_start: None,
                period_end: None,
                new_version: false,
            })
            .unwrap();
        let ris = self.directory.path().join("scopus.ris");
        std::fs::write(&ris, RIS).unwrap();
        database
            .import_strategy_results(&strategies[0].id, &ris.to_string_lossy())
            .unwrap();
        let included = self.article("10.1000/mural.1");
        let excluded = self.article("10.1000/heritage.2");
        let criteria = database
            .save_criterion(CriterionInput {
                id: None,
                project_id: project.clone(),
                criterion_type: "inclusion".into(),
                code: "I1".into(),
                name: "English".into(),
                description: String::new(),
            })
            .unwrap();
        let criteria = database
            .save_criterion(CriterionInput {
                id: None,
                project_id: project.clone(),
                criterion_type: "exclusion".into(),
                code: "E1".into(),
                name: "Tema 1".into(),
                description: String::new(),
            })
            .unwrap_or(criteria);
        let english = criteria
            .iter()
            .find(|criterion| criterion.code == "I1")
            .unwrap()
            .id
            .clone();
        let theme = criteria
            .iter()
            .find(|criterion| criterion.code == "E1")
            .unwrap()
            .id
            .clone();
        let decide = |article: &str, phase: &str, decision: &str, criterion: Option<&str>| {
            database
                .record_screening_decision(DecisionInput {
                    project_id: project.clone(),
                    article_id: article.into(),
                    phase: phase.into(),
                    decision: decision.into(),
                    criterion_id: criterion.map(str::to_owned),
                    criteria: BTreeMap::from([(english.clone(), "met".to_owned())]),
                    reason: String::new(),
                })
                .unwrap();
        };
        decide(&included, "metadata", "include", None);
        decide(&included, "full_text", "include", None);
        decide(&excluded, "metadata", "exclude", Some(&theme));

        let scale = vec![
            ScaleOption {
                label: "Sí".into(),
                value: 1.0,
            },
            ScaleOption {
                label: "No".into(),
                value: 0.0,
            },
        ];
        let quality = database
            .save_quality_criterion(QualityCriterionInput {
                id: None,
                project_id: project.clone(),
                code: None,
                question: "¿Objetivos claros?".into(),
                description: String::new(),
                weight: 2.0,
                scale,
            })
            .unwrap();
        database.save_scores_for_test(
            &project,
            &included,
            vec![ScoreEntry {
                criterion_id: quality[0].id.clone(),
                score: Some(1.0),
                justification: "Sección 1".into(),
            }],
        );

        let overview = database
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
        let field = |name: &str, rq: Option<String>| FieldInput {
            id: None,
            project_id: project.clone(),
            research_question_id: rq,
            name: name.into(),
            description: String::new(),
            data_type: "text".into(),
            unit: None,
            taxonomy: Vec::new(),
            instructions: String::new(),
            required: false,
            examples: Vec::new(),
            master_field_key: None,
        };
        database.save_extraction_field(field("País", None)).unwrap();
        database
            .save_extraction_field(field("Enfoque", Some(overview.questions[0].id.clone())))
            .unwrap();
        let overview = database
            .save_extraction_field(field(
                "Dataset & métricas",
                Some(overview.questions[0].id.clone()),
            ))
            .unwrap();
        let approach = overview.questions[0].fields[0].id.clone();
        database
            .save_extraction_value(CellInput {
                project_id: project.clone(),
                article_id: included.clone(),
                field_id: approach,
                value_id: None,
                value_text: Some("Semisupervisado 100%".into()),
                status: "confirmed".into(),
                notes: String::new(),
                evidence_id: None,
                quote: Some("Semi-supervised segmentation".into()),
                page: Some(2),
            })
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

    fn export(&self, format: &str, variant: Option<&str>) -> (ExportResult, String) {
        let path = self
            .directory
            .path()
            .join(format!("export-{format}-{}", variant.unwrap_or("x")));
        let result = self
            .database
            .export_project(ExportRequest {
                project_id: self.project_id.clone(),
                format: format.into(),
                variant: variant.map(str::to_owned),
                scope: None,
                path: path.to_string_lossy().into_owned(),
            })
            .unwrap();
        let text = if format == "xlsx" {
            String::new()
        } else {
            std::fs::read_to_string(&result.path).unwrap()
        };
        (result, text)
    }
}

impl Database {
    fn save_scores_for_test(&self, project: &str, article: &str, scores: Vec<ScoreEntry>) {
        self.save_quality_scores(project, article, scores).unwrap();
    }
}

fn row(
    workbook: &mut Xlsx<std::io::BufReader<std::fs::File>>,
    sheet: &str,
    index: usize,
) -> Vec<String> {
    let range = workbook.worksheet_range(sheet).unwrap();
    range
        .rows()
        .nth(index)
        .map(|cells| {
            cells
                .iter()
                .map(|cell| match cell {
                    Data::Empty => String::new(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn compatible_workbook_has_the_seven_sheets_and_their_headers() {
    let fixture = fixture();
    let (result, _) = fixture.export("xlsx", Some("compatible"));
    assert!(result.path.ends_with(".xlsx"), "extension added");
    let mut workbook: Xlsx<_> = open_workbook(&result.path).unwrap();
    assert_eq!(
        workbook.sheet_names(),
        vec![
            "Leer",
            "Control",
            "RQs",
            "Cadena",
            "I_E",
            "Calidad",
            "Extraccion"
        ]
    );
    assert_eq!(row(&mut workbook, "Control", 0), vec!["Id", "Reference"]);
    assert_eq!(
        row(&mut workbook, "Cadena", 0),
        vec!["Base de datos", "Fecha", "Cadena", "Cantidad"]
    );
    assert_eq!(
        row(&mut workbook, "RQs", 0),
        vec!["RQ", "Aspecto 1", "Aspecto 2"]
    );
    assert_eq!(
        row(&mut workbook, "RQs", 1),
        vec!["RQ1: ¿Qué enfoque se usa?", "Enfoque", "Dataset & métricas"]
    );
    assert_eq!(
        row(&mut workbook, "I_E", 0),
        vec![
            "Id",
            "Reference",
            "Title",
            "Keywords",
            "Abstract",
            "Year",
            "Authors",
            "Conference/Journal",
            "English",
            "Tema 1",
            "I/E"
        ]
    );
    let first = row(&mut workbook, "I_E", 1);
    assert_eq!((first[8].as_str(), first[10].as_str()), ("Sí", "I"));
    assert_eq!(row(&mut workbook, "I_E", 2)[10], "E");
    assert_eq!(
        row(&mut workbook, "Calidad", 0),
        vec!["Id", "Reference", "Title", "Year", "Q1", "Total"]
    );
    assert_eq!(
        row(&mut workbook, "Calidad", 1)[5],
        "2",
        "total = weight × score"
    );
    let groups = row(&mut workbook, "Extraccion", 0);
    assert_eq!(
        (groups[0].as_str(), groups[4].as_str(), groups[5].as_str()),
        ("Id", "General", "RQ1")
    );
    assert_eq!(
        row(&mut workbook, "Extraccion", 1)[4..],
        ["País", "Enfoque", "Dataset & métricas"]
    );
    let values = row(&mut workbook, "Extraccion", 2);
    assert_eq!(
        (values[1].as_str(), values[5].as_str()),
        ("Researcher (2024)", "Semisupervisado 100%")
    );
    assert_eq!(
        workbook
            .worksheet_range("Extraccion")
            .unwrap()
            .rows()
            .count(),
        3,
        "only included studies"
    );
}

#[test]
fn extended_workbook_adds_provenance_and_is_recorded_with_its_hash() {
    let fixture = fixture();
    let (result, _) = fixture.export("xlsx", Some("extended"));
    let mut workbook: Xlsx<_> = open_workbook(&result.path).unwrap();
    let names = workbook.sheet_names();
    assert_eq!(names.len(), 12);
    for sheet in [
        "Extraccion_procedencia",
        "I_E_decisiones",
        "Calidad_revisores",
        "Auditoria",
        "Exportacion",
    ] {
        assert!(names.iter().any(|name| name == sheet), "missing {sheet}");
    }
    let provenance = row(&mut workbook, "Extraccion_procedencia", 1);
    assert_eq!(
        (
            provenance[4].as_str(),
            provenance[5].as_str(),
            provenance[6].as_str(),
            provenance[7].as_str()
        ),
        (
            "Semisupervisado 100%",
            "Confirmado",
            "Semi-supervised segmentation",
            "2"
        )
    );
    assert_eq!(
        row(&mut workbook, "Control", 0),
        vec!["Id", "Reference", "DOI", "Justificación", "Recuperado por"]
    );
    assert_eq!(
        result.sha256,
        crate::library::sha256_file(std::path::Path::new(&result.path)).unwrap()
    );
    let history = fixture
        .database
        .export_history(&fixture.project_id)
        .unwrap();
    assert_eq!(
        (history.len(), history[0].output_sha256.as_deref()),
        (1, Some(result.sha256.as_str()))
    );
    assert_eq!(history[0].options["variant"], json!("extended"));
}

#[test]
fn bibliographies_round_trip_and_escape_special_characters() {
    let fixture = fixture();
    let (_, ris_text) = fixture.export("ris", None);
    assert_eq!(
        ris_text.matches("TY  - ").count(),
        1,
        "only included studies by default"
    );
    let other = tempdir().unwrap();
    let fresh = Database::initialize(other.path().join("fresh.sqlite3")).unwrap();
    let path = other.path().join("roundtrip.ris");
    std::fs::write(&path, &ris_text).unwrap();
    fresh
        .import_reference_files(vec![path.to_string_lossy().into_owned()])
        .unwrap();
    let articles = fresh.library_articles(None).unwrap();
    assert_eq!(
        (
            articles[0].title.as_str(),
            articles[0].doi.as_deref(),
            articles[0].publication_year
        ),
        (
            "Semi-supervised segmentation of murals & frescoes",
            Some("10.1000/mural.1"),
            Some(2024)
        )
    );

    let (_, bib) = fixture.export("bibtex", None);
    assert!(
        bib.starts_with("@article{researcher2024semisupervised,"),
        "{bib}"
    );
    assert!(bib.contains("murals \\& frescoes") && bib.contains("95\\% accuracy\\_overall"));
    assert!(bib.contains("journal = {Heritage Science}"));
}

#[test]
fn reports_csv_and_json_carry_the_matrix_and_the_evidence_annex() {
    let fixture = fixture();
    let (_, csv) = fixture.export("csv", None);
    assert!(csv.starts_with('\u{feff}'));
    assert!(csv.contains("RQ1 · Dataset & métricas") && csv.contains("Semisupervisado 100%"));
    let (_, markdown) = fixture.export("markdown", None);
    assert!(markdown.contains("## RQ1 — ¿Qué enfoque se usa?"));
    assert!(
        markdown.contains("«Semi-supervised segmentation», p. 2"),
        "evidence annex with page"
    );
    let (_, latex) = fixture.export("latex", None);
    assert!(latex.contains("Dataset \\& métricas") && latex.contains("Semisupervisado 100\\%"));
    assert!(latex.contains("\\begin{longtable}") && latex.trim_end().ends_with("\\end{document}"));
    let (_, json_text) = fixture.export("json", None);
    let snapshot: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(snapshot["format"], json!(SNAPSHOT_FORMAT));
    assert_eq!(snapshot["matrix"]["rows"].as_array().unwrap().len(), 1);
    assert!(
        fixture
            .database
            .export_project(ExportRequest {
                project_id: fixture.project_id.clone(),
                format: "pdf".into(),
                variant: None,
                scope: None,
                path: "/tmp/x.pdf".into()
            })
            .is_err()
    );
    assert!(
        fixture
            .database
            .export_project(ExportRequest {
                project_id: fixture.project_id.clone(),
                format: "csv".into(),
                variant: None,
                scope: None,
                path: "relative.csv".into()
            })
            .is_err()
    );
}
