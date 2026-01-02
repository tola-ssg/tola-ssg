//! Compiler Actor - Typst Compilation Wrapper
//!
//! This actor is responsible for:
//! 1. Receiving file change notifications
//! 2. Dispatching to `pipeline::compile` for actual compilation
//! 3. Forwarding results to VdomActor for diffing
//!
//! # Design
//!
//! This is a **thin wrapper** around `pipeline::compile`. The actor only handles:
//! - Async message loop
//! - spawn_blocking for CPU work
//! - Routing results to other actors
//!
//! All business logic lives in `pipeline::compile`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use super::messages::{CompilerMsg, VdomMsg, WsMsg};
use crate::config::SiteConfig;
use crate::pipeline::compile::{compile_page, CompileOutcome};

/// Compiler Actor - handles Typst compilation
pub struct CompilerActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<CompilerMsg>,
    /// Channel to send VDOM to VdomActor
    vdom_tx: mpsc::Sender<VdomMsg>,
    /// Channel to send reload messages directly to WsActor (for non-content changes)
    ws_tx: mpsc::Sender<WsMsg>,
    /// Site configuration
    config: Arc<SiteConfig>,
}

impl CompilerActor {
    /// Create a new CompilerActor
    pub fn new(
        rx: mpsc::Receiver<CompilerMsg>,
        vdom_tx: mpsc::Sender<VdomMsg>,
        ws_tx: mpsc::Sender<WsMsg>,
        config: Arc<SiteConfig>,
    ) -> Self {
        Self {
            rx,
            vdom_tx,
            ws_tx,
            config,
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                CompilerMsg::Compile(paths) => {
                    self.handle_compile(paths).await;
                }

                CompilerMsg::Shutdown => {
                    crate::log!("compile"; "shutting down");
                    break;
                }
            }
        }
    }

    /// Handle compilation request
    async fn handle_compile(&self, paths: Vec<PathBuf>) {
        let start = Instant::now();
        let config = Arc::clone(&self.config);

        // Spawn blocking to preserve rayon parallelism
        let results = tokio::task::spawn_blocking(move || {
            Self::do_compile(&paths, &config)
        }).await;

        match results {
            Ok(outcomes) => {
                let duration = start.elapsed();
                let count = outcomes.len();
                crate::log!("compile"; "compiled {} files in {:?}", count, duration);

                // Route results to appropriate actors
                for outcome in outcomes {
                    self.route_outcome(outcome).await;
                }
            }
            Err(e) => {
                crate::log!("compile"; "spawn_blocking error: {}", e);
            }
        }
    }

    /// Route a compile outcome to the appropriate actor
    async fn route_outcome(&self, outcome: CompileOutcome) {
        match outcome {
            CompileOutcome::Vdom { path, url_path, vdom } => {
                // Send to VdomActor for diffing
                let _ = self.vdom_tx.send(VdomMsg::Process {
                    path,
                    url_path,
                    vdom,
                }).await;
            }
            CompileOutcome::Reload { reason } => {
                // Directly notify WsActor
                let _ = self.ws_tx.send(WsMsg::Reload { reason }).await;
            }
            CompileOutcome::Skipped => {
                // Nothing to do
            }
            CompileOutcome::Error { path, error } => {
                crate::log!("compile"; "error in {}: {}", path.display(), error);
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: format!("compile error: {}", error),
                }).await;
            }
        }
    }

    /// Perform compilation (runs in blocking thread pool)
    ///
    /// Delegates to `pipeline::compile::compile_page` for each file.
    fn do_compile(paths: &[PathBuf], config: &SiteConfig) -> Vec<CompileOutcome> {
        paths.iter()
            .map(|path| compile_page(path, config))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_outcome_variants() {
        // Test non-VDOM variants (VDOM requires complex setup)
        let _ = CompileOutcome::Reload {
            reason: "test".to_string(),
        };
        let _ = CompileOutcome::Skipped;
        let _ = CompileOutcome::Error {
            path: PathBuf::from("/test.typ"),
            error: "test error".to_string(),
        };
    }
}
