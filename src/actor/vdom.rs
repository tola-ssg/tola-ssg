//! VDOM Actor - The Bridge between Typst and Hot Reload
//!
//! This actor is responsible for:
//! 1. Receiving compiled VDOM from CompilerActor
//! 2. Computing diffs via `pipeline::diff`
//! 3. Managing VDOM cache via `pipeline::cache`
//! 4. Sending patch/reload messages to WsActor
//!
//! # Design
//!
//! This is a **thin wrapper** around `pipeline::diff`. The actor only handles:
//! - Async message loop
//! - spawn_blocking for CPU work (diffing)
//! - Routing results to WsActor
//!
//! All business logic lives in `pipeline::diff` and `pipeline::cache`.

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use super::messages::{VdomMsg, WsMsg};
use crate::vdom::VdomCache;
use crate::pipeline::diff::{compute_diff, DiffOutcome};
use crate::vdom::{Document, Indexed};

/// VDOM Actor - converts AST to VDOM and computes diffs
pub struct VdomActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<VdomMsg>,
    /// Channel to send messages to WsActor
    ws_tx: mpsc::Sender<WsMsg>,
    /// VDOM cache (owned by this actor, wrapped for spawn_blocking)
    cache: Arc<Mutex<VdomCache>>,
}

impl VdomActor {
    /// Create a new VdomActor
    pub fn new(
        rx: mpsc::Receiver<VdomMsg>,
        ws_tx: mpsc::Sender<WsMsg>,
    ) -> Self {
        Self {
            rx,
            ws_tx,
            cache: Arc::new(Mutex::new(VdomCache::new())),
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                VdomMsg::Process { path, url_path, vdom } => {
                    self.handle_process(path, url_path, vdom).await;
                }

                VdomMsg::Invalidate { url_path } => {
                    self.cache.lock().remove(&url_path);
                    crate::log!("vdom"; "invalidated cache for {}", url_path);
                }

                VdomMsg::Clear => {
                    self.cache.lock().clear();
                    crate::log!("vdom"; "cleared all cache");
                }

                VdomMsg::Shutdown => {
                    crate::log!("vdom"; "shutting down");
                    break;
                }
            }
        }
    }

    /// Handle VDOM processing request
    ///
    /// Delegates to `pipeline::diff::compute_diff` for the actual work.
    async fn handle_process(
        &self,
        _path: PathBuf,
        url_path: String,
        new_vdom: Document<Indexed>,
    ) {
        let cache = Arc::clone(&self.cache);
        let url_path_clone = url_path.clone();

        // Compute diff in blocking thread (diff can be CPU intensive)
        let result = tokio::task::spawn_blocking(move || {
            let mut cache = cache.lock();
            compute_diff(&mut cache, &url_path_clone, new_vdom)
        }).await;

        // Route outcome to WsActor
        match result {
            Ok(outcome) => self.route_outcome(url_path, outcome).await,
            Err(e) => {
                crate::log!("vdom"; "spawn_blocking error: {}", e);
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: format!("internal error: {}", e),
                }).await;
            }
        }
    }

    /// Route a diff outcome to WsActor
    async fn route_outcome(&self, url_path: String, outcome: DiffOutcome) {
        match outcome {
            DiffOutcome::Patches(patches) => {
                let count = patches.len();
                crate::log!("vdom"; "patch {} ({} ops)", url_path, count);
                let _ = self.ws_tx.send(WsMsg::Patch { url_path, patches }).await;
            }
            DiffOutcome::Initial => {
                // First compile after server start - cache was empty.
                // Browser already has HTML loaded, but we have no old VDOM to diff against.
                // Trigger reload to ensure browser shows latest content.
                crate::log!("vdom"; "initial {} (reload)", url_path);
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: "initial compile".to_string(),
                }).await;
            }
            DiffOutcome::Unchanged => {
                crate::log!("vdom"; "unchanged {}", url_path);
            }
            DiffOutcome::NeedsReload { reason } => {
                crate::log!("vdom"; "reload {}: {}", url_path, reason);
                let _ = self.ws_tx.send(WsMsg::Reload { reason }).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vdom_cache_via_pipeline() {
        let cache = VdomCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get("/test").is_none());
    }
}
