//! Compiler Actor
//!
//! Receives compile requests from FsActor, performs compilation using rayon,
//! and sends patches/reload messages to WsActor.

use std::path::PathBuf;
use std::time::Instant;

use tokio::sync::mpsc;

use super::messages::{CompilerMsg, WsMsg};

/// Compiler Actor - handles compilation and diffing
pub struct CompilerActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<CompilerMsg>,
    /// Channel to send messages to WsActor
    ws_tx: mpsc::Sender<WsMsg>,
    /// Previous VDOM state for diffing (placeholder)
    #[allow(dead_code)]
    previous_vdom: Option<()>, // TODO: Replace with actual VDOM cache
}

impl CompilerActor {
    /// Create a new CompilerActor
    pub fn new(
        rx: mpsc::Receiver<CompilerMsg>,
        ws_tx: mpsc::Sender<WsMsg>,
    ) -> Self {
        Self {
            rx,
            ws_tx,
            previous_vdom: None,
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                CompilerMsg::Compile(paths) => {
                    let start = Instant::now();

                    // Spawn blocking to preserve rayon parallelism
                    let result = tokio::task::spawn_blocking(move || {
                        Self::do_compile(&paths)
                    }).await;

                    match result {
                        Ok(Ok(patches)) => {
                            let duration = start.elapsed();
                            crate::log!("compile"; "compiled {} files in {:?}",
                                patches.len(), duration);

                            if patches.is_empty() {
                                // No changes, trigger reload as fallback
                                let _ = self.ws_tx.send(WsMsg::Reload {
                                    reason: "files changed".to_string(),
                                }).await;
                            } else {
                                let _ = self.ws_tx.send(WsMsg::Patch(patches)).await;
                            }
                        }
                        Ok(Err(e)) => {
                            crate::log!("compile"; "error: {}", e);
                            let _ = self.ws_tx.send(WsMsg::Reload {
                                reason: format!("compile error: {}", e),
                            }).await;
                        }
                        Err(e) => {
                            crate::log!("compile"; "spawn_blocking error: {}", e);
                        }
                    }
                }

                CompilerMsg::Shutdown => {
                    crate::log!("compile"; "shutting down");
                    break;
                }
            }
        }
    }

    /// Perform compilation (runs in blocking thread pool)
    fn do_compile(paths: &[PathBuf]) -> anyhow::Result<Vec<crate::hotreload::StableIdPatch>> {
        // TODO: Integrate with actual compilation pipeline
        // For now, this is a placeholder that returns empty patches

        crate::log!("compile"; "compiling {} files", paths.len());

        // Placeholder: In real implementation, this would:
        // 1. Compile changed files using typst
        // 2. Convert to VDOM
        // 3. Diff against previous VDOM
        // 4. Return patch operations

        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Placeholder test
        assert!(true);
    }
}
