//! Compiler Actor - Typst Compilation Wrapper
//!
//! This actor is responsible for:
//! 1. Receiving file change notifications
//! 2. Dispatching to `pipeline::compile` for actual compilation
//! 3. Forwarding ALL results to VdomActor (linear message flow)
//!
//! # Design
//!
//! This is a **thin wrapper** around `pipeline::compile`. The actor only handles:
//! - Async message loop
//! - spawn_blocking for CPU work
//! - Routing results to VdomActor (NEVER directly to WsActor)
//!
//! All business logic lives in `pipeline::compile`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;

use super::messages::{CompilerMsg, VdomMsg};
use crate::config::SiteConfig;
use crate::logger::WatchStatus;
use crate::pipeline::compile::{compile_page, CompileOutcome};

/// Compiler Actor - handles Typst compilation
pub struct CompilerActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<CompilerMsg>,
    /// Channel to send ALL results to VdomActor (linear message flow)
    vdom_tx: mpsc::Sender<VdomMsg>,
    /// Site configuration
    config: Arc<SiteConfig>,
    /// Status display for watch mode
    status: WatchStatus,
}

impl CompilerActor {
    /// Create a new CompilerActor
    pub fn new(
        rx: mpsc::Receiver<CompilerMsg>,
        vdom_tx: mpsc::Sender<VdomMsg>,
        config: Arc<SiteConfig>,
    ) -> Self {
        Self {
            rx,
            vdom_tx,
            config,
            status: WatchStatus::new(),
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                CompilerMsg::Compile(paths) => {
                    self.handle_compile(paths).await;
                }

                CompilerMsg::CompileDependents(deps) => {
                    // Find content files that depend on these deps and compile them
                    let affected = Self::collect_dependents(&deps);
                    if affected.is_empty() {
                        crate::log!("compile"; "no dependents found for {} deps", deps.len());
                    } else {
                        self.handle_compile(affected).await;
                    }
                }

                CompilerMsg::FullRebuild => {
                    self.handle_full_rebuild().await;
                }

                CompilerMsg::Shutdown => {
                    crate::log!("compile"; "shutting down");
                    break;
                }
            }
        }
    }

    /// Collect content files that depend on the given dependency files
    fn collect_dependents(deps: &[PathBuf]) -> Vec<PathBuf> {
        use crate::compiler::deps::get_dependents;
        use rustc_hash::FxHashSet;

        let mut affected = FxHashSet::default();

        for dep in deps {
            affected.extend(get_dependents(dep.as_path()));
        }

        affected.into_iter().collect()
    }

    /// Handle full site rebuild (config changed)
    async fn handle_full_rebuild(&self) {
        use crate::compiler::deps::clear_graph;
        use crate::driver::Development;

        crate::log!("compile"; "full rebuild triggered");

        // Clear dependency graph
        clear_graph();

        // Clear VDOM cache
        let _ = self.vdom_tx.send(VdomMsg::Clear).await;

        // Perform full build
        let config = Arc::clone(&self.config);
        let result = tokio::task::spawn_blocking(move || {
            crate::build::build_site(Development, &config, true)
        }).await;

        match result {
            Ok(Ok(_)) => {
                crate::log!("compile"; "full rebuild complete");
                let _ = self.vdom_tx.send(VdomMsg::Reload {
                    reason: "full rebuild".to_string(),
                }).await;
            }
            Ok(Err(e)) => {
                crate::log!("compile"; "full rebuild failed: {}", e);
                let _ = self.vdom_tx.send(VdomMsg::Reload {
                    reason: format!("rebuild failed: {}", e),
                }).await;
            }
            Err(e) => {
                crate::log!("compile"; "spawn_blocking error: {}", e);
                let _ = self.vdom_tx.send(VdomMsg::Reload {
                    reason: format!("internal error: {}", e),
                }).await;
            }
        }
    }

    /// Handle compilation request
    async fn handle_compile(&mut self, paths: Vec<PathBuf>) {
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
                self.status.success(&format!("compiled {} files in {:?}", count, duration));

                // Route ALL results to VdomActor (linear message flow)
                for outcome in outcomes {
                    self.route_outcome(outcome).await;
                }
            }
            Err(e) => {
                self.status.error("spawn_blocking error", &e.to_string());
                // Forward error to VdomActor
                let _ = self.vdom_tx.send(VdomMsg::Reload {
                    reason: format!("internal error: {}", e),
                }).await;
            }
        }
    }

    /// Route a compile outcome to VdomActor
    ///
    /// All outcomes go through VdomActor to ensure linear message flow.
    /// VdomActor is the sole decision maker for what to send to WsActor.
    async fn route_outcome(&mut self, outcome: CompileOutcome) {
        match outcome {
            CompileOutcome::Vdom { path, url_path, vdom } => {
                let _ = self.vdom_tx.send(VdomMsg::Process {
                    path,
                    url_path,
                    vdom,
                }).await;
            }
            CompileOutcome::Reload { reason } => {
                let _ = self.vdom_tx.send(VdomMsg::Reload { reason }).await;
            }
            CompileOutcome::Skipped => {
                let _ = self.vdom_tx.send(VdomMsg::Skip).await;
            }
            CompileOutcome::Error { path, error } => {
                self.status.error(&format!("compile error in {}", path.display()), &error);
                let _ = self.vdom_tx.send(VdomMsg::Reload {
                    reason: format!("compile error in {}: {}", path.display(), error),
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
