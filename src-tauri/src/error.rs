use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Error de base de datos: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Error de sistema de archivos: {0}")]
    Io(#[from] std::io::Error),
    #[error("Datos inválidos: {0}")]
    Validation(String),
    #[error("No se pudo inicializar la aplicación: {0}")]
    Initialization(String),
    #[error("Extractor: {0}")]
    Worker(String),
    #[error("IA local: {0}")]
    Ai(String),
    #[error("Operación cancelada")]
    Cancelled,
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
