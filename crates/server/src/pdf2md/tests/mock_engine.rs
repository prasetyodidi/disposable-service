//! Test double for `ExtractionEngine`. Registered via `EngineRegistry::register("mock", ...)`
//! or placed directly in `PdfRuntime.engine`.
#![allow(dead_code)]
use crate::pdf2md::engine::{ExtractRequest, ExtractionEngine};
use crate::pdf2md::error::EngineError;
use crate::pdf2md::model::IntermediateDoc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What `MockEngine::extract` does on each call.
#[derive(Debug, Clone)]
pub(crate) enum MockOutcome {
    /// Return this document.
    Doc(IntermediateDoc),
    /// Return this engine error.
    Err(EngineError),
    /// Sleep for the duration (async, cancellable by dropping the future),
    /// then return `EngineError::Timeout`.
    Sleep(Duration),
}

pub(crate) struct MockEngine {
    pub outcome: Mutex<MockOutcome>,
    pub calls: AtomicUsize,
}

impl MockEngine {
    pub fn new(outcome: MockOutcome) -> Self {
        Self {
            outcome: Mutex::new(outcome),
            calls: AtomicUsize::new(0),
        }
    }

    pub fn arc(outcome: MockOutcome) -> Arc<MockEngine> {
        Arc::new(Self::new(outcome))
    }

    pub fn doc(doc: IntermediateDoc) -> Arc<MockEngine> {
        Self::arc(MockOutcome::Doc(doc))
    }

    pub fn err(e: EngineError) -> Arc<MockEngine> {
        Self::arc(MockOutcome::Err(e))
    }

    pub fn sleep(d: Duration) -> Arc<MockEngine> {
        Self::arc(MockOutcome::Sleep(d))
    }

    /// Number of times `extract` has been entered.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    pub fn set_outcome(&self, outcome: MockOutcome) {
        *self.outcome.lock().unwrap() = outcome;
    }
}

#[axum::async_trait]
impl ExtractionEngine for MockEngine {
    fn name(&self) -> &'static str {
        "mock"
    }

    async fn extract(&self, _req: &ExtractRequest<'_>) -> Result<IntermediateDoc, EngineError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let outcome = self.outcome.lock().unwrap().clone();
        match outcome {
            MockOutcome::Doc(d) => Ok(d),
            MockOutcome::Err(e) => Err(e),
            MockOutcome::Sleep(d) => {
                tokio::time::sleep(d).await;
                Err(EngineError::Timeout)
            }
        }
    }
}
