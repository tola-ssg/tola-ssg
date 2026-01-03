//! VDOM Actor - The Bridge between Typst and Hot Reload
//!
//! This actor is responsible for:
//! 1. Receiving compiled VDOM from CompilerActor
//! 2. Computing diffs via `pipeline::diff`
//! 3. Managing VDOM cache via `pipeline::diff`
//! 4. Sending patch/reload messages to WsActor
//!
//! # Design
//!
//! This is a **thin wrapper** around `pipeline::diff`. The actor only handles:
//! - Async message loop
//! - spawn_blocking for CPU work (diffing)
//! - Routing results to WsActor
//!

use crate::logger::WatchStatus;

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tokio::sync::mpsc;

use super::messages::{VdomMsg, WsMsg};
use crate::hotreload::logic::diff::{compute_diff, DiffOutcome};
use crate::vdom::{CacheKey, VdomCache};
use crate::vdom::{Document, Indexed};

/// VDOM Actor - converts AST to VDOM and computes diffs
pub struct VdomActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<VdomMsg>,
    /// Channel to send messages to WsActor
    ws_tx: mpsc::Sender<WsMsg>,
    /// VDOM cache (owned by this actor, wrapped for spawn_blocking)
    cache: Arc<Mutex<VdomCache>>,
    /// Status display for watch mode
    status: WatchStatus,
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
            status: WatchStatus::new(),
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                VdomMsg::Populate { entries } => {
                    // Pre-fill cache with initial build results
                    let count = entries.len();
                    let mut cache = self.cache.lock();
                    for (url_path, vdom) in entries {
                        let key = CacheKey::new(&url_path);
                        cache.insert(key, vdom);
                    }
                    crate::log!("vdom"; "populated cache with {} entries", count);
                }

                VdomMsg::Process { path, url_path, vdom } => {
                    self.handle_process(path, url_path, vdom).await;
                }

                VdomMsg::Reload { reason } => {
                    // Forward reload from CompilerActor to WsActor
                    self.status.success(&format!("reload: {}", reason));
                    let _ = self.ws_tx.send(WsMsg::Reload { reason }).await;
                }

                VdomMsg::Skip => {
                    // Skipped files (drafts, etc.) don't require any action
                }

                VdomMsg::Invalidate { url_path } => {
                    let key = CacheKey::new(&url_path);
                    self.cache.lock().remove(&key);
                    self.status.success(&format!("invalidated cache for {}", url_path));
                }

                VdomMsg::Clear => {
                    self.cache.lock().clear();
                    self.status.success("cleared all cache");
                }

                VdomMsg::Shutdown => {
                    self.status.success("vdom actor shutting down");
                    break;
                }
            }
        }
    }

    /// Handle VDOM processing request
    ///
    /// Delegates to `pipeline::diff::compute_diff` for the actual work.
    async fn handle_process(
        &mut self,
        _path: PathBuf,
        url_path: String,
        new_vdom: Document<Indexed>,
    ) {
        let cache = Arc::clone(&self.cache);
        let key = CacheKey::new(&url_path);

        // Compute diff in blocking thread (diff can be CPU intensive)
        let result = tokio::task::spawn_blocking(move || {
            let mut cache = cache.lock();
            compute_diff(&mut cache, key, new_vdom)
        }).await;

        // Route outcome to WsActor
        match result {
            Ok(outcome) => self.route_outcome(url_path, outcome).await,
            Err(e) => {
                self.status.error("spawn_blocking error", &e.to_string());
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: format!("internal error: {}", e),
                }).await;
            }
        }
    }

    /// Route a diff outcome to WsActor
    async fn route_outcome(&mut self, url_path: String, outcome: DiffOutcome) {
        match outcome {
            DiffOutcome::Patches(patches, new_vdom) => {
                let count = patches.len();
                self.status.success(&format!("patch {} ({} ops)", url_path, count));

                // Send patches to WsActor
                if self.ws_tx.send(WsMsg::Patch { url_path: url_path.clone(), patches }).await.is_ok() {
                    // Update cache AFTER successful send
                    // This keeps cache in sync with what browser should display
                    let key = CacheKey::new(&url_path);
                    self.cache.lock().insert(key, new_vdom);
                }
            }
            DiffOutcome::Initial => {
                // First compile after server start - cache was empty.
                // Browser already has HTML loaded, but we have no old VDOM to diff against.
                // Trigger reload to ensure browser shows latest content.
                self.status.success(&format!("initial {}", url_path));
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: "initial compile".to_string(),
                }).await;
            }
            DiffOutcome::Unchanged => {
                self.status.unchanged(&url_path);
            }
            DiffOutcome::NeedsReload { reason } => {
                self.status.success(&format!("reload {}: {}", url_path, reason));
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
        let key = CacheKey::new("/test");
        assert!(cache.get(&key).is_none());
    }
}
