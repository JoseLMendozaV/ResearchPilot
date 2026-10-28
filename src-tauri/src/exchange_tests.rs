//! Two reviewers on two machines exchanging packages.

use std::collections::BTreeMap;

use serde_json::json;
use tempfile::{TempDir, tempdir};

use super::*;
use crate::{
    matrix::CellInput,
    models::CreateProjectInput,
    review::{FieldInput, ProjectSettingsInput, QuestionInput, StrategyInput},
    screening::{CriterionInput, DecisionInput},
};

const RIS: &str = "TY  - JOUR\nTI  - Semi-supervised segmentation of murals\nAU  - Researcher, Ana\nPY  - 2024\nDO  - 10.1000/mural.1\nER  - \n\nTY  - JOUR\nTI  - Continual learning for heritage images\nPY  - 2023\nDO  - 10.1000/heritage.2\nER  - \n\nTY  - JOUR\nTI  - Anomaly detection in old photographs\nPY  - 2022\nDO  - 10.1000/photo.3\nER  - \n";

struct Machine {
    directory: TempDir,
    database: Database,
}

fn machine(researcher: &str) -> Machine {
    let directory = tempdir().unwrap();
    let database = Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
    database
        .open()
        .unwrap()
        .execute(
            "UPDATE app_settings SET researcher_name = ?1 WHERE id = 1",
            [researcher],
        )
        .unwrap();
    Machine {
        directory,
        database,
    }
}

impl Machine {
    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.path().join(name)
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

    fn decide(&self, project: &str, article: &str, decision: &str, criterion: Option<&str>) {
        self.database
            .record_screening_decision(DecisionInput {
                project_id: project.into(),
                article_id: article.into(),
                phase: "metadata".into(),
                decision: decision.into(),
                criterion_id: criterion.map(str::to_owned),
                criteria: BTreeMap::new(),
                reason: String::new(),
            })
            .unwrap();
    }

    fn stage(&self, project: &str, article: &str) -> String {
        self.database
            .open()
            .unwrap()
            .query_row(
                "SELECT workflow_stage FROM review_articles WHERE project_id = ?1 AND article_id = ?2",
                [project, article],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn extract(&self, project: &str, article: &str, field: &str, value: &str) {
        self.database
            .save_extraction_value(CellInput {
                project_id: project.into(),
                article_id: article.into(),
                field_id: field.into(),
                value_id: None,
                value_text: Some(value.into()),
                status: "confirmed".into(),
                notes: String::new(),
                evidence_id: None,
                quote: Some("Segmentation with few labels".into()),
                page: None,
            })
            .unwrap();
    }

    fn open_conflicts(&self, project: &str) -> Vec<ConflictKind> {
        let mut kinds: Vec<ConflictKind> = self
            .database
            .conflicts(project)
            .unwrap()
            .into_iter()
            .filter(|conflict| conflict.status == "open")
            .map(|conflict| (conflict.conflict_type, conflict.title))
            .collect();
        kinds.sort();
        kinds
    }
}

type ConflictKind = (String, String);

/// Ana sets the review up: criteria, double screening, one extraction field.
fn setup(ana: &Machine) -> (String, String, String) {
    let project = ana
        .database
        .create_project(CreateProjectInput {
            name: "Mapeo compartido".into(),
            description: String::new(),
            review_type: "systematic_mapping".into(),
        })
        .unwrap()
        .id;
    let workspace = ana.database.project_workspace(&project).unwrap();
    ana.database
        .update_project_settings(ProjectSettingsInput {
            id: project.clone(),
            name: workspace.project.name,
            description: String::new(),
            review_type: workspace.project.review_type,
            status: "active".into(),
            screening_mode: "double".into(),
            quality_threshold: None,
            quality_missing_policy: "zero".into(),
        })
        .unwrap();
    let criterion = ana
        .database
        .save_criterion(CriterionInput {
            id: None,
            project_id: project.clone(),
            criterion_type: "exclusion".into(),
            code: "E1".into(),
            name: "No aborda patrimonio".into(),
            description: String::new(),
        })
        .unwrap()[0]
        .id
        .clone();
    let strategy = ana
        .database
        .save_search_strategy(StrategyInput {
            id: None,
            project_id: project.clone(),
            database_name: "Scopus".into(),
            query_text: "heritage AND segmentation".into(),
            query_date: "2026-10-01".into(),
            result_count: 3,
            notes: String::new(),
            filters: json!({}),
            language: None,
            period_start: None,
            period_end: None,
            new_version: false,
        })
        .unwrap()[0]
        .id
        .clone();
    let ris = ana.path("scopus.ris");
    fs::write(&ris, RIS).unwrap();
    ana.database
        .import_strategy_results(&strategy, &ris.to_string_lossy())
        .unwrap();
    let rq = ana
        .database
        .save_research_question(QuestionInput {
            id: None,
            project_id: project.clone(),
            parent_id: None,
            code: None,
            question: "¿Qué enfoque de aprendizaje se usa?".into(),
            objective: String::new(),
            category: "Técnicas".into(),
        })
        .unwrap()
        .questions[0]
        .id
        .clone();
    let field = ana
        .database
        .save_extraction_field(FieldInput {
            id: None,
            project_id: project.clone(),
            research_question_id: Some(rq),
            name: "Enfoque".into(),
            description: String::new(),
            data_type: "category".into(),
            unit: None,
            taxonomy: vec!["Supervisado".into(), "Semisupervisado".into()],
            instructions: String::new(),
            required: false,
            examples: Vec::new(),
            master_field_key: None,
        })
        .unwrap()
        .questions[0]
        .fields[0]
        .id
        .clone();
    (project, criterion, field)
}

#[test]
fn deferred_double_review_round_trip() {
    let ana = machine("Ana");
    let (project, e1, field) = setup(&ana);
    let (a1, a2, a3) = (
        ana.article("10.1000/mural.1"),
        ana.article("10.1000/heritage.2"),
        ana.article("10.1000/photo.3"),
    );
    ana.decide(&project, &a1, "include", None);
    ana.decide(&project, &a2, "include", None);
    ana.decide(&project, &a3, "exclude", Some(&e1));
    ana.extract(&project, &a1, &field, "Supervisado");
    assert_eq!(
        ana.stage(&project, &a1),
        "screening",
        "double review waits for a second reviewer"
    );

    let from_ana = ana.path("ana.json");
    let exported = ana
        .database
        .export_exchange_package(&project, &from_ana)
        .unwrap();
    assert_eq!(
        (exported.articles, exported.decisions, exported.values),
        (3, 3, 1)
    );

    // Bruno already has one of the papers in his library under another id.
    let bruno = machine("Bruno");
    let seed = bruno.path("seed.ris");
    fs::write(&seed, "TY  - JOUR\nTI  - Semi-supervised segmentation of murals\nPY  - 2024\nDO  - 10.1000/MURAL.1\nER  - \n").unwrap();
    bruno
        .database
        .import_reference_files(vec![seed.to_string_lossy().into()])
        .unwrap();
    let local_b1 = bruno.article("10.1000/mural.1");
    assert_ne!(local_b1, a1);

    let summary = bruno.database.import_exchange_package(&from_ana).unwrap();
    assert!(summary.project_created);
    assert_eq!((summary.articles_matched, summary.articles_created), (1, 2));
    assert_eq!((summary.decisions_added, summary.values_added), (3, 1));
    assert_eq!(summary.open_conflicts, 0);
    let again = bruno.database.import_exchange_package(&from_ana).unwrap();
    assert!(
        again.already_imported,
        "the same package twice changes nothing"
    );

    // Same study ids, criteria and reviewers on both sides.
    let workspace = bruno.database.project_workspace(&project).unwrap();
    assert_eq!(workspace.project.screening_mode, "double");
    let names: Vec<_> = workspace
        .reviewers
        .iter()
        .map(|r| (r.display_name.clone(), r.is_local_profile))
        .collect();
    assert!(
        names.contains(&("Bruno".into(), true)) && names.contains(&("Ana".into(), false)),
        "{names:?}"
    );
    let sequence =
        |machine: &Machine, article: &str| -> i64 {
            machine.database.open().unwrap().query_row(
            "SELECT sequence FROM review_articles WHERE project_id = ?1 AND article_id = ?2",
            [&project, article],
            |row| row.get(0),
        ).unwrap()
        };
    assert_eq!(sequence(&bruno, &local_b1), sequence(&ana, &a1));
    assert_eq!(sequence(&bruno, &a2), sequence(&ana, &a2));

    // Bruno reviews independently: agrees on 1 and 3, disagrees on 2.
    bruno.decide(&project, &local_b1, "include", None);
    bruno.decide(&project, &a2, "exclude", Some(&e1));
    bruno.decide(&project, &a3, "exclude", Some(&e1));
    bruno.extract(&project, &local_b1, &field, "Semisupervisado");
    assert_eq!(bruno.stage(&project, &local_b1), "eligible");
    assert_eq!(bruno.stage(&project, &a3), "excluded");

    let from_bruno = bruno.path("bruno.json");
    bruno
        .database
        .export_exchange_package(&project, &from_bruno)
        .unwrap();
    let summary = ana.database.import_exchange_package(&from_bruno).unwrap();
    assert_eq!(
        summary.decisions_added, 3,
        "Ana's own decisions are not duplicated"
    );
    assert_eq!(summary.values_added, 1);
    assert_eq!(summary.articles_created, 0);
    assert_eq!(ana.stage(&project, &a1), "eligible");
    assert_eq!(ana.stage(&project, &a2), "screening");
    assert_eq!(ana.stage(&project, &a3), "excluded");
    let conflicts = ana.open_conflicts(&project);
    assert_eq!(
        conflicts,
        vec![
            (
                "extraction".into(),
                "Semi-supervised segmentation of murals".into()
            ),
            (
                "screening".into(),
                "Continual learning for heritage images".into()
            ),
        ]
    );

    // Ana resolves both conflicts; Bruno receives the resolutions.
    let all = ana.database.conflicts(&project).unwrap();
    let screening_conflict = all.iter().find(|c| c.conflict_type == "screening").unwrap();
    ana.database
        .resolve_screening_conflict(
            &screening_conflict.id,
            "exclude",
            Some(e1.clone()),
            "Fuera del alcance acordado",
        )
        .unwrap();
    let extraction_conflict = all
        .iter()
        .find(|c| c.conflict_type == "extraction")
        .unwrap();
    let values = extraction_conflict.details["values"].as_array().unwrap();
    let ana_value = values.iter().find(|v| v["reviewer"] == "Ana").unwrap()["valueId"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        ana.database
            .resolve_extraction_conflict(&extraction_conflict.id, "otro", "Ok")
            .is_err(),
        "the kept value must belong to the conflict"
    );
    ana.database
        .resolve_extraction_conflict(
            &extraction_conflict.id,
            &ana_value,
            "Se usan etiquetas completas",
        )
        .unwrap();
    assert!(ana.open_conflicts(&project).is_empty());
    assert_eq!(ana.stage(&project, &a2), "excluded");
    let statuses: Vec<String> = ana
        .database
        .open()
        .unwrap()
        .prepare("SELECT status FROM extracted_values ORDER BY status")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        statuses,
        vec!["confirmed", "rejected"],
        "the other value is kept as rejected"
    );

    assert_eq!(
        bruno.open_conflicts(&project).len(),
        1,
        "Bruno saw the screening disagreement too"
    );
    let resolved = ana.path("ana-2.json");
    ana.database
        .export_exchange_package(&project, &resolved)
        .unwrap();
    bruno.database.import_exchange_package(&resolved).unwrap();
    assert_eq!(bruno.stage(&project, &a2), "excluded");
    assert!(
        bruno
            .open_conflicts(&project)
            .iter()
            .all(|(kind, _)| kind != "screening")
    );

    let history = ana.database.exchange_history(&project).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["reviewerName"], "Bruno");
}

#[test]
fn exchange_requires_distinct_named_reviewers_and_valid_packages() {
    let unnamed = machine("");
    let (project, _, _) = setup(&unnamed);
    let error = unnamed
        .database
        .export_exchange_package(&project, &unnamed.path("x.json"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("Perfil local"), "{error}");

    let ana = machine("Ana");
    let (project, _, _) = setup(&ana);
    let package = ana.path("ana.json");
    ana.database
        .export_exchange_package(&project, &package)
        .unwrap();
    let namesake = machine("Ana");
    let error = namesake
        .database
        .import_exchange_package(&package)
        .unwrap_err()
        .to_string();
    assert!(error.contains("mismo nombre"), "{error}");
    assert!(
        namesake.database.projects().unwrap().is_empty(),
        "nothing was written"
    );

    let other = ana.path("otro.json");
    fs::write(&other, r#"{"hello": "world"}"#).unwrap();
    assert!(ana.database.import_exchange_package(&other).is_err());

    // Column names in a package never reach SQL unless they exist locally.
    let mut tampered: ExchangePackage =
        serde_json::from_slice(&fs::read(&package).unwrap()).unwrap();
    tampered.tables.get_mut("research_questions").unwrap()[0]
        .insert("x\"); DROP TABLE articles; --".into(), json!(1));
    let path = ana.path("tampered.json");
    fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
    let bruno = machine("Bruno");
    bruno.database.import_exchange_package(&path).unwrap();
    assert_eq!(bruno.database.library_articles(None).unwrap().len(), 3);
}

#[test]
fn a_pdf_imported_later_attaches_to_the_metadata_only_record() {
    let ana = machine("Ana");
    let pdf = ana.path("paper.pdf");
    crate::library::tests::create_pdf(&pdf);
    let imported = ana
        .database
        .import_pdf_files(vec![pdf.to_string_lossy().into()])
        .unwrap();
    let article = imported[0].article_id.clone().unwrap();
    let project = ana
        .database
        .create_project(CreateProjectInput {
            name: "Con PDF".into(),
            description: String::new(),
            review_type: "systematic_review".into(),
        })
        .unwrap()
        .id;
    ana.database
        .add_to_corpus(&project, std::slice::from_ref(&article), None)
        .unwrap();
    let package = ana.path("ana.json");
    ana.database
        .export_exchange_package(&project, &package)
        .unwrap();

    let bruno = machine("Bruno");
    bruno.database.import_exchange_package(&package).unwrap();
    assert!(
        bruno
            .database
            .article_by_id(&article)
            .unwrap()
            .document_path
            .is_none()
    );
    let result = bruno
        .database
        .import_pdf_files(vec![pdf.to_string_lossy().into()])
        .unwrap();
    assert_eq!(result[0].status, "imported");
    assert_eq!(result[0].article_id.as_deref(), Some(article.as_str()));
    assert!(
        bruno
            .database
            .article_by_id(&article)
            .unwrap()
            .document_path
            .is_some()
    );
    assert_eq!(bruno.database.library_articles(None).unwrap().len(), 1);
}
