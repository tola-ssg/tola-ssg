//! VDOM Actor - The Bridge between Typst and Hot Reload
//!
//! This actor is responsible for:
//! 1. Converting Typst AST to Tola VDOM (TTG conversion)
//! 2. Running the VDOM Pipeline (Index → Process → Render)
//! 3. Computing diffs against cached previous state
//! 4. Managing VDOM cache (internal state, no global)
//!
//! # Architecture
//!
//! ```text
//! CompilerActor ──[Artifacts]──► VdomActor ──[Patch/Reload]──► WsActor
//!                                    │
//!                              VdomCache
//!                           (internal state)
//! ```
//!
//! # Message Flow
//!
//! 1. Receives `VdomMsg::Process { path, html }` from CompilerActor
//! 2. Converts HTML to VDOM, runs pipeline
//! 3. Diffs against cached VDOM
//! 4. Sends `WsMsg::Patch` or `WsMsg::Reload` to WsActor

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use rustc_hash::FxHashMap;
use tokio::sync::mpsc;

use super::messages::{VdomMsg, WsMsg};
use crate::vdom::diff::{diff, DiffResult as VdomDiffResult, Patch};
use crate::vdom::{Document, Indexed};

// =============================================================================
// Types
// =============================================================================

/// VDOM cache - stores previous VDOM state for diffing
///
/// This replaces the global `VDOM_CACHE` static. Each VdomActor owns
/// its cache, eliminating shared mutable state.
#[derive(Debug, Default)]
pub struct VdomCache {
    /// Maps URL path to cached VDOM document
    pages: FxHashMap<String, Document<Indexed>>,
}

impl VdomCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Get cached VDOM for a URL path
    pub fn get(&self, url_path: &str) -> Option<&Document<Indexed>> {
        self.pages.get(url_path)
    }

    /// Insert or update cached VDOM, returns the old value if any
    pub fn insert(&mut self, url_path: String, doc: Document<Indexed>) -> Option<Document<Indexed>> {
        self.pages.insert(url_path, doc)
    }

    /// Remove a page from cache
    pub fn remove(&mut self, url_path: &str) -> Option<Document<Indexed>> {
        self.pages.remove(url_path)
    }

    /// Clear all cached pages
    pub fn clear(&mut self) {
        self.pages.clear();
    }

    /// Number of cached pages
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

/// Internal outcome of diff computation
#[derive(Debug)]
enum DiffOutcome {
    /// First time seeing this page, no diff possible
    Initial,
    /// No changes detected
    Unchanged,
    /// Patches to apply
    Patches(Vec<Patch>),
    /// Structural change requires full reload
    NeedsReload { reason: String },
}

// =============================================================================
// VdomActor
// =============================================================================

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

            let outcome = if let Some(old_vdom) = cache.get(&url_path_clone) {
                let diff_result: VdomDiffResult = diff(old_vdom, &new_vdom);

                if diff_result.should_reload {
                    DiffOutcome::NeedsReload {
                        reason: diff_result.reload_reason.unwrap_or_else(|| "complex change".to_string()),
                    }
                } else if diff_result.ops.is_empty() {
                    DiffOutcome::Unchanged
                } else {
                    DiffOutcome::Patches(diff_result.ops)
                }
            } else {
                DiffOutcome::Initial
            };

            // Update cache with new VDOM
            cache.insert(url_path_clone, new_vdom);

            outcome
        }).await;

        match result {
            Ok(DiffOutcome::Patches(patches)) => {
                let count = patches.len();
                crate::log!("vdom"; "patch {} ({} ops)", url_path, count);
                let _ = self.ws_tx.send(WsMsg::Patch { url_path, patches }).await;
            }
            Ok(DiffOutcome::Initial) => {
                crate::log!("vdom"; "initial {} (no diff)", url_path);
                // First compile - could send full HTML or just skip
            }
            Ok(DiffOutcome::Unchanged) => {
                crate::log!("vdom"; "unchanged {}", url_path);
            }
            Ok(DiffOutcome::NeedsReload { reason }) => {
                crate::log!("vdom"; "reload {}: {}", url_path, reason);
                let _ = self.ws_tx.send(WsMsg::Reload { reason }).await;
            }
            Err(e) => {
                crate::log!("vdom"; "spawn_blocking error: {}", e);
                let _ = self.ws_tx.send(WsMsg::Reload {
                    reason: format!("internal error: {}", e),
                }).await;
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vdom_cache_empty() {
        let cache = VdomCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(cache.get("/test").is_none());
    }
}
