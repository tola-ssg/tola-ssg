//! Compiler Actor - Typst Compilation Wrapper
//!
//! This actor is responsible for:
//! 1. Receiving file change notifications
//! 2. Compiling Typst files to VDOM
//! 3. Forwarding results to VdomActor for diffing
//!
//! # Architecture
//!
//! ```text
//! FsActor ──[Compile(paths)]──► CompilerActor ──[Process]──► VdomActor
//!                                    │
//!                              typst::World
//!                           (compilation state)
//! ```
//!
//! # Responsibility Boundary
//!
//! - **This Actor**: Typst compilation only (AST → VDOM)
//! - **NOT This Actor**: Diffing, caching, broadcasting (handled by VdomActor/WsActor)

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use super::messages::{CompilerMsg, VdomMsg, WsMsg};
use crate::compiler::pages::process_page;
use crate::config::SiteConfig;
use crate::driver::Development;
use crate::vdom::Indexed;

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

/// Result from compiling a single file
#[derive(Debug)]
enum CompileOutcome {
    /// Successfully compiled to VDOM
    Vdom {
        path: PathBuf,
        url_path: String,
        vdom: crate::vdom::Document<Indexed>,
    },
    /// Non-content file changed, needs reload
    Reload { reason: String },
    /// File skipped (draft, not found, etc.)
    Skipped,
    /// Compilation error
    Error { path: PathBuf, error: String },
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
            }
            Err(e) => {
                crate::log!("compile"; "spawn_blocking error: {}", e);
            }
        }
    }

    /// Perform Typst compilation (runs in blocking thread pool)
    ///
    /// This function:
    /// 1. Filters files by type
    /// 2. Compiles .typ files to VDOM
    /// 3. Returns outcomes for routing
    ///
    /// Note: No diffing or caching here - that's VdomActor's job
    fn do_compile(paths: &[PathBuf], config: &SiteConfig) -> Vec<CompileOutcome> {
        let mut outcomes = Vec::with_capacity(paths.len());

        for path in paths {
            let ext = path.extension().and_then(|e| e.to_str());

            match ext {
                Some("typ") => {
                    // Compile Typst file
                    match Self::compile_typst_file(path, config) {
                        Ok(Some((url_path, vdom))) => {
                            outcomes.push(CompileOutcome::Vdom {
                                path: path.clone(),
                                url_path,
                                vdom,
                            });
                        }
                        Ok(None) => {
                            outcomes.push(CompileOutcome::Skipped);
                        }
                        Err(e) => {
                            outcomes.push(CompileOutcome::Error {
                                path: path.clone(),
                                error: e.to_string(),
                            });
                        }
                    }
                }
                Some("css" | "js" | "html") => {
                    // Asset file - needs reload
                    outcomes.push(CompileOutcome::Reload {
                        reason: format!("asset changed: {}", path.display()),
                    });
                }
                _ => {
                    // Unknown file type - trigger reload to be safe
                    outcomes.push(CompileOutcome::Reload {
                        reason: format!("file changed: {}", path.display()),
                    });
                }
            }
        }

        outcomes
    }

    /// Compile a single Typst file to VDOM
    fn compile_typst_file(
        path: &PathBuf,
        config: &SiteConfig,
    ) -> anyhow::Result<Option<(String, crate::vdom::Document<Indexed>)>> {
        // Use existing process_page with Development driver instance
        let driver = Development;
        let result = process_page(&driver, path, config)?;

        match result {
            Some(page_result) => {
                // Extract URL path and VDOM
                let url_path = page_result.url_path;

                // indexed_vdom is only populated in development mode
                if let Some(vdom) = page_result.indexed_vdom {
                    Ok(Some((url_path, vdom)))
                } else {
                    // No VDOM available (shouldn't happen in dev mode)
                    Ok(None)
                }
            }
            None => {
                // Page was skipped (draft, etc.)
                Ok(None)
            }
        }
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
