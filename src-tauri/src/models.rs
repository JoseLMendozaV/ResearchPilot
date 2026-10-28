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
