//! End-to-end methodology flow on a temporary database.

use std::collections::BTreeMap;

use serde_json::json;
use tempfile::{TempDir, tempdir};

use super::*;
use crate::{
    matrix::CellInput,
    models::{AddMasterValueInput, CreateProjectInput},
    quality::{QualityCriterionInput, ScaleOption, ScoreEntry},
    screening::{CriterionInput, DecisionInput},
};

const RIS: &str = "TY  - JOUR\nTI  - Semi-supervised segmentation of murals\nAU  - Researcher, Ana\nPY  - 2024\nDO  - 10.1000/mural.1\nAB  - Segmentation with few labels.\nER  - \n\nTY  - JOUR\nTI  - Continual learning for heritage images\nAU  - Scientist, Luis\nPY  - 2023\nDO  - 10.1000/heritage.2\nER  - \n\nTY  - JOUR\nTI  - Anomaly detection in old photographs\nAU  - Analyst, Carla\nPY  - 2022\nDO  - 10.1000/photo.3\nER  - \n";

struct Fixture {
    directory: TempDir,
    database: Database,
    project_id: String,
}

fn fixture() -> Fixture {
    let directory = tempdir().unwrap();
    let database = Database::initialize(directory.path().join("researchpilot.sqlite3")).unwrap();
    let project = database
        .create_project(CreateProjectInput {
            name: "Mapeo de patrimonio".into(),
            description: String::new(),
            review_type: "systematic_mapping".into(),
        })
        .unwrap();
    Fixture {
        directory,
        database,
        project_id: project.id,
    }
}

impl Fixture {
    fn file(&self, name: &str, content: &str) -> String {
        let path = self.directory.path().join(name);
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn strategy(&self, database_name: &str) -> String {
        let strategies = self
            .database
            .save_search_strategy(StrategyInput {
                id: None,
                project_id: self.project_id.clone(),
                database_name: database_name.into(),
                query_text: "(heritage OR mural) AND segmentation".into(),
                query_date: "2026-10-01".into(),
                result_count: 3,
                notes: String::new(),
                filters: json!({ "language": "en" }),
                language: Some("en".into()),
                period_start: Some(2015),
                period_end: Some(2026),
                new_version: false,
            })
            .unwrap();
        strategies
            .iter()
            .find(|strategy| strategy.database_name == database_name)
            .unwrap()
            .id
            .clone()
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

    fn decide(
        &self,
        article_id: &str,
        phase: &str,
        decision: &str,
        criterion: Option<&str>,
        reason: &str,
    ) -> AppResult<crate::screening::ScreeningRow> {
        self.database.record_screening_decision(DecisionInput {
            project_id: self.project_id.clone(),
            article_id: article_id.into(),
            phase: phase.into(),
            decision: decision.into(),
            criterion_id: criterion.map(str::to_owned),
            criteria: BTreeMap::new(),
            reason: reason.into(),
        })
    }
}

#[test]
fn protocol_versions_require_amendments_after_approval() {
    let fixture = fixture();
    let project = &fixture.project_id;
    fixture
        .database
        .save_research_question(QuestionInput {
            id: None,
            project_id: project.clone(),
            parent_id: None,
            code: None,
            question: "¿Qué técnicas se usan?".into(),
            objective: String::new(),
            category: "Técnicas".into(),
        })
        .unwrap();
    fixture
        .database
        .save_protocol(
            project,
            json!({ "title": "Protocolo", "objectives": "Mapear técnicas" }),
            None,
        )
        .unwrap();
    let versions = fixture.database.approve_protocol(project).unwrap();
    assert_eq!(
        (versions[0].version_number, versions[0].status.as_str()),
        (1, "approved")
    );
    assert_eq!(
        versions[0].content["snapshot"]["researchQuestions"][0]["code"],
        json!("RQ1"),
        "approval freezes the RQs"
    );

    assert!(
        fixture
            .database
            .save_protocol(
                project,
                json!({ "title": "Protocolo", "objectives": "Otro" }),
                None
            )
            .is_err(),
        "approved protocols change only through amendments"
    );
    let versions = fixture
        .database
        .save_protocol(
            project,
            json!({ "title": "Protocolo", "objectives": "Otro" }),
            Some("Se amplía el objetivo".into()),
        )
        .unwrap();
    assert_eq!(
        (versions[0].version_number, versions[0].status.as_str()),
        (2, "draft")
    );
    assert_eq!(
        versions[0].amendments[0].change["changedSections"],
        json!(["objectives"])
    );
    let versions = fixture.database.approve_protocol(project).unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|version| version.status.as_str())
            .collect::<Vec<_>>(),
        vec!["approved", "superseded"]
    );
}

#[test]
fn search_imports_build_the_corpus_and_measure_control_coverage() {
    let fixture = fixture();
    let project = fixture.project_id.clone();
    // A known key paper already in the library becomes a control article.
    let seed = fixture.file("seed.ris", "TY  - JOUR\nTI  - Semi-supervised segmentation of murals\nPY  - 2024\nDO  - 10.1000/mural.1\nER  - \n");
    fixture.database.import_reference_files(vec![seed]).unwrap();
    let control_article = fixture.article("10.1000/mural.1");
    let coverage = fixture
        .database
        .add_control_article(&project, &control_article, "Artículo clave del área")
        .unwrap();
    assert_eq!((coverage.total, coverage.found), (1, 0));

    let scopus = fixture.strategy("Scopus");
    let result = fixture
        .database
        .import_strategy_results(&scopus, &fixture.file("scopus.ris", RIS))
        .unwrap();
    assert_eq!(
        (
            result.records,
            result.new_articles,
            result.duplicates,
            result.added_to_corpus
        ),
        (3, 2, 1, 3)
    );
    let again = fixture
        .database
        .import_strategy_results(&scopus, &fixture.file("scopus-copy.ris", RIS))
        .unwrap();
    assert!(again.file_already_imported, "same content, same batch");
    assert_eq!(again.added_to_corpus, 0);

    let coverage = fixture.database.control_coverage(&project).unwrap();
    assert_eq!(
        (coverage.found, coverage.articles[0].found_by.clone()),
        (1, vec!["Scopus v1".to_owned()])
    );

    let strategies = fixture.database.search_strategies(&project).unwrap();
    assert_eq!(strategies[0].records_imported, 3);
    let versions = fixture
        .database
        .save_search_strategy(StrategyInput {
            id: Some(scopus.clone()),
            project_id: project.clone(),
            database_name: "Scopus".into(),
            query_text: "mural AND segmentation".into(),
            query_date: "2026-10-15".into(),
            result_count: 2,
            notes: String::new(),
            filters: json!({}),
            language: None,
            period_start: None,
            period_end: None,
            new_version: true,
        })
        .unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|strategy| strategy.version_number)
            .collect::<Vec<_>>(),
        vec![2, 1]
    );

    let table = fixture
        .database
        .screening_table(&project, "metadata")
        .unwrap();
    assert_eq!(
        table
            .rows
            .iter()
            .map(|row| row.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3],
        "stable sequential Ids"
    );
    assert_eq!(table.rows[0].source_database.as_deref(), Some("Scopus"));
}

#[test]
fn screening_single_and_double_review_with_conflicts_and_agreement() {
    let fixture = fixture();
    let project = fixture.project_id.clone();
    let scopus = fixture.strategy("Scopus");
    fixture
        .database
        .import_strategy_results(&scopus, &fixture.file("r.ris", RIS))
        .unwrap();
    let (first, second, third) = (
        fixture.article("10.1000/mural.1"),
        fixture.article("10.1000/heritage.2"),
        fixture.article("10.1000/photo.3"),
    );
    let criteria = fixture
        .database
        .save_criterion(CriterionInput {
            id: None,
            project_id: project.clone(),
            criterion_type: "exclusion".into(),
            code: "E1".into(),
            name: "No aborda patrimonio".into(),
            description: String::new(),
        })
        .unwrap();
    let e1 = criteria[0].id.clone();

    assert!(
        fixture
            .decide(&first, "metadata", "exclude", None, "")
            .is_err(),
        "exclusions need a criterion or a reason"
    );
    assert_eq!(
        fixture
            .decide(&first, "metadata", "include", None, "")
            .unwrap()
            .stage,
        "eligible"
    );
    assert!(
        fixture
            .decide(&first, "full_text", "exclude", None, "no aplica")
            .is_err(),
        "full-text exclusions need the reason criterion"
    );
    assert_eq!(
        fixture
            .decide(&first, "full_text", "include", None, "")
            .unwrap()
            .stage,
        "included"
    );
    assert_eq!(
        fixture
            .decide(&third, "metadata", "exclude", Some(&e1), "")
            .unwrap()
            .stage,
        "excluded"
    );

    // Double review: the second reviewer's view decides when they agree.
    let mut settings = fixture.database.project_workspace(&project).unwrap();
    fixture
        .database
        .update_project_settings(ProjectSettingsInput {
            id: project.clone(),
            name: settings.project.name.clone(),
            description: String::new(),
            review_type: settings.project.review_type.clone(),
            status: "active".into(),
            screening_mode: "double".into(),
            quality_threshold: Some(60.0),
            quality_missing_policy: "zero".into(),
        })
        .unwrap();
    settings = fixture
        .database
        .add_project_reviewer(&project, "Revisora B", "reviewer")
        .unwrap();
    let local = settings
        .reviewers
        .iter()
        .find(|reviewer| reviewer.is_local_profile)
        .unwrap()
        .id
        .clone();
    let other = settings
        .reviewers
        .iter()
        .find(|reviewer| !reviewer.is_local_profile)
        .unwrap()
        .id
        .clone();

    let row = fixture
        .decide(&second, "metadata", "include", None, "")
        .unwrap();
    assert!(
        row.final_decision.is_none(),
        "one reviewer is not enough in double mode"
    );
    fixture
        .database
        .set_active_reviewer(&project, &other)
        .unwrap();
    let row = fixture
        .decide(&second, "metadata", "exclude", Some(&e1), "")
        .unwrap();
    assert!(row.conflict_id.is_some() && row.final_decision.is_none());
    assert_eq!(
        fixture
            .database
            .project_workspace(&project)
            .unwrap()
            .open_conflicts,
        1
    );
    let resolved = fixture
        .database
        .resolve_screening_conflict(
            row.conflict_id.as_ref().unwrap(),
            "include",
            None,
            "Cumple el tema tras leer el resumen",
        )
        .unwrap();
    assert_eq!(
        (
            resolved.final_decision.unwrap().decision.as_str(),
            resolved.stage.as_str()
        ),
        ("include", "eligible")
    );

    // Agreement in full text finalizes automatically.
    fixture
        .decide(&second, "full_text", "include", None, "")
        .unwrap();
    fixture
        .database
        .set_active_reviewer(&project, &local)
        .unwrap();
    let row = fixture
        .decide(&second, "full_text", "include", None, "")
        .unwrap();
    assert_eq!(row.stage, "included");
    assert_eq!(
        row.final_decision.unwrap().reviewer_name,
        "Acuerdo entre revisores"
    );

    let table = fixture
        .database
        .screening_table(&project, "full_text")
        .unwrap();
    let agreement = table.agreement.unwrap();
    assert_eq!((agreement.compared, agreement.observed), (1, 1.0));
    assert!(agreement.note.contains("Muestra pequeña"));
    let conflicts = fixture.database.conflicts(&project).unwrap();
    assert_eq!(conflicts[0].status, "resolved");
}

#[test]
fn quality_totals_follow_the_scale_and_double_review_needs_consensus() {
    let fixture = fixture();
    let project = fixture.project_id.clone();
    let scopus = fixture.strategy("Scopus");
    fixture
        .database
        .import_strategy_results(&scopus, &fixture.file("r.ris", RIS))
        .unwrap();
    let article = fixture.article("10.1000/mural.1");
    fixture
        .decide(&article, "metadata", "include", None, "")
        .unwrap();
    let scale = vec![
        ScaleOption {
            label: "Sí".into(),
            value: 1.0,
        },
        ScaleOption {
            label: "Parcial".into(),
            value: 0.5,
        },
        ScaleOption {
            label: "No".into(),
            value: 0.0,
        },
    ];
    let save = |question: &str, weight: f64| QualityCriterionInput {
        id: None,
        project_id: project.clone(),
        code: None,
        question: question.into(),
        description: String::new(),
        weight,
        scale: scale.clone(),
    };
    fixture
        .database
        .save_quality_criterion(save("¿Los objetivos están claros?", 1.0))
        .unwrap();
    let criteria = fixture
        .database
        .save_quality_criterion(save("¿La evaluación es reproducible?", 2.0))
        .unwrap();
    assert_eq!(
        criteria
            .iter()
            .map(|criterion| criterion.code.as_str())
            .collect::<Vec<_>>(),
        vec!["Q1", "Q2"]
    );
    let entry = |index: usize, score: Option<f64>| ScoreEntry {
        criterion_id: criteria[index].id.clone(),
        score,
        justification: String::new(),
    };

    assert!(
        fixture
            .database
            .save_quality_scores(&project, &article, vec![entry(0, Some(0.7))])
            .is_err(),
        "scores must belong to the scale"
    );
    let table = fixture
        .database
        .save_quality_scores(
            &project,
            &article,
            vec![entry(0, Some(1.0)), entry(1, Some(0.5))],
        )
        .unwrap();
    assert_eq!(table.max_total, 3.0);
    let consensus = table.rows[0].final_assessment.clone().unwrap();
    assert_eq!((consensus.total, consensus.max_total), (2.0, 3.0));
    assert_eq!(
        table.rows[0].meets_threshold, None,
        "no threshold configured"
    );

    let workspace = fixture.database.project_workspace(&project).unwrap();
    fixture
        .database
        .update_project_settings(ProjectSettingsInput {
            id: project.clone(),
            name: workspace.project.name,
            description: String::new(),
            review_type: workspace.project.review_type,
            status: "active".into(),
            screening_mode: "double".into(),
            quality_threshold: Some(70.0),
            quality_missing_policy: "exclude_from_max".into(),
        })
        .unwrap();
    let workspace = fixture
        .database
        .add_project_reviewer(&project, "Revisor C", "reviewer")
        .unwrap();
    let other = workspace
        .reviewers
        .iter()
        .find(|reviewer| !reviewer.is_local_profile)
        .unwrap()
        .id
        .clone();
    fixture
        .database
        .set_active_reviewer(&project, &other)
        .unwrap();
    let table = fixture
        .database
        .save_quality_scores(
            &project,
            &article,
            vec![entry(0, Some(1.0)), entry(1, Some(1.0))],
        )
        .unwrap();
    assert!(
        table.rows[0].conflict_id.is_some(),
        "different scores open a conflict"
    );
    assert!(
        table.rows[0].final_assessment.is_none(),
        "no consensus while in conflict"
    );
    let table = fixture
        .database
        .resolve_quality_conflict(
            table.rows[0].conflict_id.as_ref().unwrap(),
            vec![entry(0, Some(1.0)), entry(1, None)],
            "Q2 no se puede valorar",
        )
        .unwrap();
    let consensus = table.rows[0].final_assessment.clone().unwrap();
    assert_eq!(
        (consensus.total, consensus.max_total),
        (1.0, 1.0),
        "missing answers leave the maximum"
    );
    assert_eq!(table.rows[0].meets_threshold, Some(true));
}

#[test]
fn extraction_matrix_cells_are_typed_traceable_and_prefilled_from_confirmed_values() {
    let fixture = fixture();
    let project = fixture.project_id.clone();
    let scopus = fixture.strategy("Scopus");
    fixture
        .database
        .import_strategy_results(&scopus, &fixture.file("r.ris", RIS))
        .unwrap();
    let included = fixture.article("10.1000/mural.1");
    let pending = fixture.article("10.1000/heritage.2");
    fixture
        .decide(&included, "metadata", "include", None, "")
        .unwrap();
    fixture
        .decide(&included, "full_text", "include", None, "")
        .unwrap();

    let overview = fixture
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
        .unwrap();
    let rq = overview.questions[0].id.clone();
    let field = |name: &str,
                 data_type: &str,
                 taxonomy: Vec<String>,
                 rq: Option<String>,
                 master: Option<&str>| FieldInput {
        id: None,
        project_id: project.clone(),
        research_question_id: rq,
        name: name.into(),
        description: String::new(),
        data_type: data_type.into(),
        unit: None,
        taxonomy,
        instructions: String::new(),
        required: false,
        examples: Vec::new(),
        master_field_key: master.map(str::to_owned),
    };
    assert!(
        fixture
            .database
            .save_extraction_field(field(
                "Enfoque",
                "category",
                Vec::new(),
                Some(rq.clone()),
                None
            ))
            .is_err(),
        "categories need a taxonomy"
    );
    fixture
        .database
        .save_extraction_field(field(
            "Enfoque",
            "category",
            vec!["Supervisado".into(), "Semisupervisado".into()],
            Some(rq.clone()),
            None,
        ))
        .unwrap();
    let overview = fixture
        .database
        .save_extraction_field(field(
            "Tamaño de muestra",
            "number",
            Vec::new(),
            None,
            Some("design_methods.sample_size"),
        ))
        .unwrap();
    let approach = overview.questions[0].fields[0].id.clone();

    let matrix = fixture.database.extraction_matrix(&project, false).unwrap();
    assert_eq!(
        matrix
            .rows
            .iter()
            .map(|row| row.article_id.as_str())
            .collect::<Vec<_>>(),
        vec![included.as_str()],
        "only included studies"
    );
    assert_eq!(
        matrix
            .groups
            .iter()
            .map(|group| group.label.as_str())
            .collect::<Vec<_>>(),
        vec!["General", "RQ1"]
    );
    assert_eq!(matrix.rows[0].reference, "Researcher (2024)");

    let cell = |value: &str, status: &str| CellInput {
        project_id: project.clone(),
        article_id: included.clone(),
        field_id: approach.clone(),
        value_id: None,
        value_text: Some(value.into()),
        status: status.into(),
        notes: String::new(),
        evidence_id: None,
        quote: Some("Segmentation with few labels".into()),
        page: None,
    };
    assert!(
        fixture
            .database
            .save_extraction_value(cell("Otro", "confirmed"))
            .is_err()
    );
    assert!(
        fixture
            .database
            .save_extraction_value(cell("", "confirmed"))
            .is_err(),
        "confirmed needs a value"
    );
    let values = fixture
        .database
        .save_extraction_value(cell("semisupervisado", "confirmed"))
        .unwrap();
    assert_eq!(
        (
            values[0].display.as_str(),
            values[0].evidence[0].quote.as_str()
        ),
        ("Semisupervisado", "Segmentation with few labels")
    );
    assert!(
        fixture
            .database
            .save_extraction_value(CellInput {
                article_id: pending.clone(),
                ..cell("Supervisado", "confirmed")
            })
            .is_ok(),
        "eligible studies can be extracted when shown"
    );

    // A human-confirmed master value fills the mapped aspect as a proposal.
    fixture
        .database
        .add_master_value(AddMasterValueInput {
            article_id: included.clone(),
            field_key: "design_methods.sample_size".into(),
            value_text: Some("120".into()),
            status: "confirmed".into(),
            quote: Some("120 participants".into()),
            page: Some(3),
            notes: String::new(),
        })
        .unwrap();
    assert_eq!(
        fixture
            .database
            .prefill_matrix_from_master(&project)
            .unwrap(),
        1
    );
    assert_eq!(
        fixture
            .database
            .prefill_matrix_from_master(&project)
            .unwrap(),
        0,
        "filled cells are not overwritten"
    );
    let matrix = fixture.database.extraction_matrix(&project, false).unwrap();
    let sample_field = &matrix.groups[0].fields[0].id;
    let sample = &matrix.rows[0].cells[sample_field][0];
    assert_eq!(
        (
            sample.status.as_str(),
            sample.actor_name.as_str(),
            sample.display.as_str()
        ),
        ("proposed", "Ficha maestra", "120")
    );
    assert_eq!(sample.evidence[0].quote, "120 participants");

    let actions: Vec<String> = fixture
        .database
        .project_audit(&project, 200)
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect();
    for action in [
        "results_imported",
        "decided",
        "created",
        "prefilled_from_master_record",
    ] {
        assert!(
            actions.iter().any(|item| item == action),
            "missing audit action {action}"
        );
    }
}
