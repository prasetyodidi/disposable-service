//! PDF to Markdown conversion core (no axum dependency).
pub mod config;
pub mod convert;
pub mod engine;
pub mod error;
pub mod model;
pub mod options;
pub mod poppler;
pub mod proc;
pub mod structure;
pub mod xml;


use config::{ConfigError, PdfConfig};
use engine::{EngineRegistry, ExtractionEngine, RegistryError, ENV_ENGINE};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

/// Startup failure: invalid configuration or unknown engine name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupError {
    Config(ConfigError),
    Engine(RegistryError),
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartupError::Config(e) => write!(f, "{e}"),
            StartupError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for StartupError {}

impl From<ConfigError> for StartupError {
    fn from(e: ConfigError) -> Self {
        StartupError::Config(e)
    }
}

impl From<RegistryError> for StartupError {
    fn from(e: RegistryError) -> Self {
        StartupError::Engine(e)
    }
}

/// Immutable runtime for the PDF to Markdown mini app, built once at startup
/// and stored in `AppState.pdf`.
#[derive(Clone)]
pub struct PdfRuntime {
    pub config: PdfConfig,
    pub engine: Arc<dyn ExtractionEngine>,
    /// Parent directory of per-request job directories.
    pub job_root: PathBuf,
}

impl PdfRuntime {
    /// Reads `PDF_MD_*` from the process environment.
    pub fn from_env() -> Result<PdfRuntime, StartupError> {
        Self::from_env_with(&|k| std::env::var(k).ok())
    }

    /// Same as `from_env` with an injectable environment reader.
    pub fn from_env_with(get: &dyn Fn(&str) -> Option<String>) -> Result<PdfRuntime, StartupError> {
        let config = PdfConfig::from_env_with(get)?;
        let engine = EngineRegistry::with_builtin().select(get(ENV_ENGINE).as_deref())?;
        Ok(PdfRuntime {
            config,
            engine,
            job_root: crate::state::workspace_tmp_root(),
        })
    }

    /// Default configuration with the `poppler` engine.
    pub fn defaults() -> PdfRuntime {
        PdfRuntime {
            config: PdfConfig::defaults(),
            engine: EngineRegistry::with_builtin()
                .select(None)
                .expect("poppler is always registered"),
            job_root: crate::state::workspace_tmp_root(),
        }
    }
}

#[cfg(test)]
mod tests;
