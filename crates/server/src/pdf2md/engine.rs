//! Pluggable extraction engine interface and the engine registry.
//!
//! The Structure_Builder only knows `IntermediateDoc`; engines map their own
//! output to that model.
use super::error::EngineError;
use super::model::IntermediateDoc;
use super::poppler::PopplerEngine;
use super::proc::Deadline;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::Arc;

/// Name of the environment variable that selects the engine.
pub const ENV_ENGINE: &str = "PDF_MD_ENGINE";
/// Engine used when `PDF_MD_ENGINE` is unset, empty or whitespace.
pub const DEFAULT_ENGINE: &str = "poppler";

pub struct ExtractRequest<'a> {
    /// Input PDF inside the request's temporary directory.
    pub pdf_path: &'a Path,
    /// The same temporary directory; used for `HOME`/`XDG_*`.
    pub work_dir: &'a Path,
    /// >= 1 (already validated).
    pub page_from: u32,
    /// `None` = last page; >= `page_from` when present.
    pub page_to: Option<u32>,
    pub max_pages: u32,
    pub deadline: Deadline,
    pub max_output_bytes: usize,
}

#[axum::async_trait]
pub trait ExtractionEngine: Send + Sync {
    fn name(&self) -> &'static str;
    async fn extract(&self, req: &ExtractRequest<'_>) -> Result<IntermediateDoc, EngineError>;
}

/// Raised when `PDF_MD_ENGINE` names an unregistered engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryError {
    pub given: String,
    pub valid: Vec<String>,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{ENV_ENGINE}='{}' tidak dikenal. Nilai valid: {}",
            self.given,
            self.valid.join(", ")
        )
    }
}

impl std::error::Error for RegistryError {}

#[derive(Default)]
pub struct EngineRegistry {
    engines: BTreeMap<String, Arc<dyn ExtractionEngine>>,
}

impl EngineRegistry {
    /// Registry with the built-in `poppler` engine.
    pub fn with_builtin() -> Self {
        let mut r = EngineRegistry::default();
        r.register(DEFAULT_ENGINE, Arc::new(PopplerEngine::default()));
        r
    }

    pub fn register(&mut self, name: &str, e: Arc<dyn ExtractionEngine>) {
        self.engines.insert(name.trim().to_lowercase(), e);
    }

    pub fn valid_names(&self) -> Vec<String> {
        self.engines.keys().cloned().collect()
    }

    /// `None`, empty or whitespace-only selects `poppler`; otherwise the value
    /// is trimmed and lowercased and must match a registered name exactly.
    pub fn select(&self, raw: Option<&str>) -> Result<Arc<dyn ExtractionEngine>, RegistryError> {
        let trimmed = raw.map(str::trim).unwrap_or("");
        let key = if trimmed.is_empty() {
            DEFAULT_ENGINE.to_string()
        } else {
            trimmed.to_lowercase()
        };
        self.engines.get(&key).cloned().ok_or_else(|| RegistryError {
            given: key,
            valid: self.valid_names(),
        })
    }
}
