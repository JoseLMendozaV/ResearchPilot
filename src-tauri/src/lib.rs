mod bibliography;
mod db;
mod error;
mod library;
mod models;

use db::Database;
use error::{AppError, AppResult};
use models::{
    AppSettings, BootstrapData, CreateProjectInput, ImportItemResult, LibraryArticle,
    LibraryCollection, ReviewProject, UpdateArticleMetadataInput,
};
use tauri::{Manager, State};

#[tauri::command]
fn bootstrap_app(database: State<'_, Database>) -> AppResult<BootstrapData> {
    database.bootstrap()
}

#[tauri::command]
fn create_review_project(
    database: State<'_, Database>,
    input: CreateProjectInput,
) -> AppResult<ReviewProject> {
    database.create_project(input)
}

#[tauri::command]
fn save_app_settings(
    database: State<'_, Database>,
    settings: AppSettings,
) -> AppResult<AppSettings> {
    database.update_settings(settings)
}

#[tauri::command]
fn import_pdf_files(
    database: State<'_, Database>,
    paths: Vec<String>,
) -> AppResult<Vec<ImportItemResult>> {
    database.import_pdf_files(paths)
}

#[tauri::command]
fn import_reference_files(
    database: State<'_, Database>,
    paths: Vec<String>,
) -> AppResult<Vec<ImportItemResult>> {
    database.import_reference_files(paths)
}

#[tauri::command]
fn search_library(database: State<'_, Database>, query: String) -> AppResult<Vec<LibraryArticle>> {
    database.library_articles(Some(&query))
}

#[tauri::command]
fn update_article_metadata(
    database: State<'_, Database>,
    input: UpdateArticleMetadataInput,
) -> AppResult<LibraryArticle> {
    database.update_article_metadata(input)
}

#[tauri::command]
fn archive_library_article(database: State<'_, Database>, article_id: String) -> AppResult<()> {
    database.archive_article(&article_id)
}

#[tauri::command]
fn create_library_collection(
    database: State<'_, Database>,
    name: String,
) -> AppResult<LibraryCollection> {
    database.create_library_collection(&name)
}

#[tauri::command]
fn set_article_collection(
    database: State<'_, Database>,
    article_id: String,
    collection_id: String,
    included: bool,
) -> AppResult<()> {
    database.set_article_collection(&article_id, &collection_id, included)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| AppError::Initialization(error.to_string()))?;
            let database = Database::initialize(app_data_dir.join("researchpilot.sqlite3"))?;
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            create_review_project,
            save_app_settings,
            import_pdf_files,
            import_reference_files,
            search_library,
            update_article_metadata,
            archive_library_article,
            create_library_collection,
            set_article_collection
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
