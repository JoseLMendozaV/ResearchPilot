use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub researcher_name: String,
    pub default_language: String,
    pub appearance: String,
    pub ai_provider: String,
    pub local_only: bool,
    pub allow_external_processing: bool,
    pub ollama_base_url: String,
    pub ollama_model: String,
    pub auto_extract_on_import: bool,
    pub ocr_enabled: bool,
    pub ocr_languages: String,
    pub grobid_enabled: bool,
    pub grobid_url: String,
    pub extraction_timeout_seconds: i64,
    pub extractor_python_path: String,
    pub ollama_embedding_model: String,
    pub ollama_extraction_model: String,
    pub auto_index_after_extraction: bool,
    pub rag_mode: String,
    pub rag_top_k: i64,
    pub rag_context_tokens: i64,
    pub ai_timeout_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReviewProject {
    pub id: String,
    pub name: String,
    pub description: String,
    pub review_type: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
    pub description: String,
    pub review_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryArticle {
    pub id: String,
    pub title: String,
    pub abstract_text: Option<String>,
    pub publication_year: Option<i64>,
    pub doi: Option<String>,
    pub authors: Vec<String>,
    pub keywords: Vec<String>,
    pub venue: Option<String>,
    pub document_path: Option<String>,
    pub file_name: Option<String>,
    pub document_sha256: Option<String>,
    pub byte_size: Option<i64>,
    pub page_count: i64,
    pub imported_at: String,
    pub updated_at: String,
    pub collection_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryCollection {
    pub id: String,
    pub name: String,
    pub description: String,
    pub article_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportItemResult {
    pub source_path: String,
    pub status: String,
    pub article_id: Option<String>,
    pub title: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateArticleMetadataInput {
    pub id: String,
    pub title: String,
    pub abstract_text: Option<String>,
    pub publication_year: Option<i64>,
    pub doi: Option<String>,
    pub authors: Vec<String>,
    pub keywords: Vec<String>,
    pub venue: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub projects: i64,
    pub articles: i64,
    pub research_questions: i64,
    pub evidence_spans: i64,
    pub active_jobs: i64,
    pub values_to_review: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapData {
    pub settings: AppSettings,
    pub projects: Vec<ReviewProject>,
    pub stats: DashboardStats,
    pub database_path: String,
    pub articles: Vec<LibraryArticle>,
    pub collections: Vec<LibraryCollection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobSummary {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub progress: f64,
    pub stage: Option<String>,
    pub message: Option<String>,
    pub error_message: Option<String>,
    pub article_id: Option<String>,
    pub article_title: Option<String>,
    pub attempts: i64,
    pub max_attempts: i64,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    pub article_id: String,
    pub status: String,
    pub job_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractorStatus {
    pub python_path: Option<String>,
    pub python_source: Option<String>,
    pub worker_root: String,
    pub worker_present: bool,
    pub app_environment_ready: bool,
    pub capabilities: Option<serde_json::Value>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterField {
    pub field_key: String,
    pub group_key: String,
    pub label_es: String,
    pub label_en: String,
    pub data_type: String,
    pub multi_valued: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceItem {
    pub id: String,
    pub quote: String,
    pub page: Option<i64>,
    pub bbox: Option<Vec<f64>>,
    pub page_width: Option<f64>,
    pub page_height: Option<f64>,
    pub section: Option<String>,
    pub relation: String,
    pub verified: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterValue {
    pub id: String,
    pub field_key: String,
    pub value: serde_json::Value,
    pub display_value: String,
    pub value_type: String,
    pub unit: Option<String>,
    pub origin: String,
    pub method: Option<String>,
    pub status: String,
    pub notes: String,
    pub revision_number: i64,
    pub reviewer_name: Option<String>,
    pub reviewed_at: Option<String>,
    pub extraction_run_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub prompt_version: Option<String>,
    pub conflict: bool,
    pub created_at: String,
    pub updated_at: String,
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueRevision {
    pub revision_number: i64,
    pub value: serde_json::Value,
    pub display_value: String,
    pub status: String,
    pub notes: String,
    pub actor_type: String,
    pub actor_name: String,
    pub reason: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionRunSummary {
    pub id: String,
    pub status: String,
    pub provider: String,
    pub model: Option<String>,
    pub prompt_version: String,
    pub extractor_version: Option<String>,
    pub engine: Option<String>,
    pub source_document_hash: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error_message: Option<String>,
    pub stats: serde_json::Value,
    pub warnings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableFigure {
    pub id: String,
    pub object_type: String,
    pub label: Option<String>,
    pub caption: Option<String>,
    pub page_number: Option<i64>,
    pub bbox: Option<Vec<f64>>,
    pub content: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceEntry {
    pub id: String,
    pub ordinal: Option<i64>,
    pub raw_reference: String,
    pub doi: Option<String>,
    pub page_number: Option<i64>,
    pub matched_article_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionEntry {
    pub title: String,
    pub canonical: Option<String>,
    pub page_number: i64,
    pub bbox: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    pub total: i64,
    pub extracted: i64,
    pub ocr: i64,
    pub without_text: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleExtraction {
    pub fields: Vec<MasterField>,
    pub values: Vec<MasterValue>,
    pub runs: Vec<ExtractionRunSummary>,
    pub active_job: Option<JobSummary>,
    pub tables_figures: Vec<TableFigure>,
    pub references: Vec<ReferenceEntry>,
    pub sections: Vec<SectionEntry>,
    pub pages: PageSummary,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMasterValueInput {
    pub value_id: String,
    pub value_text: Option<String>,
    pub status: String,
    pub notes: String,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddMasterValueInput {
    pub article_id: String,
    pub field_key: String,
    pub value_text: Option<String>,
    pub status: String,
    pub quote: Option<String>,
    pub page: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceListItem {
    pub id: String,
    pub article_id: String,
    pub article_title: String,
    pub value_id: String,
    pub field_key: String,
    pub field_label: String,
    pub value_status: String,
    pub display_value: String,
    pub quote: String,
    pub page: Option<i64>,
    pub created_at: String,
}
