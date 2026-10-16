use serde::Serialize;
use std::fmt;

/// Error que ve la interfaz: un código traducible (`vault.wrongPassword`…) y, a veces, un dato sin nada sensible.
/// Nunca lleva rutas, secretos ni trazas.
#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct AppError {
    pub code: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: &'static str) -> Self {
        Self { code, detail: None }
    }
    pub fn with(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: Some(detail.into()) }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code)
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        AppError::new(match e.kind() {
            PermissionDenied => "io.denied",
            NotFound => "io.notFound",
            StorageFull => "io.full",
            _ => "io.failed",
        })
    }
}

pub type Res<T> = Result<T, AppError>;
