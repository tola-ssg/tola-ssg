//! FileSystem Actor
//!
//! Watches for file changes and sends debounced events to the CompilerActor.
//! Implements the "Watcher-First" pattern for zero event loss.
//!
//! # Responsibility
//!
//! This actor is a **thin wrapper** that:
//! 1. Receives file system events from notify
//! 2. Debounces rapid changes
//! 3. Delegates classification to `pipeline::classify`
//! 4. Routes messages to CompilerActor

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use super::messages::CompilerMsg;
use crate::config::SiteConfig;
use crate::hotreload::logic::classify::{classify_changes, ClassifyResult};
use crate::logger::WatchStatus;
use crate::utils::path::normalize_path;

/// Debounce configuration
const DEBOUNCE_MS: u64 = 300;
const REBUILD_COOLDOWN_MS: u64 = 800;

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if path is a temp/backup file (editor artifacts).
fn is_temp_file(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    matches!(ext, "bck" | "bak" | "backup" | "swp" | "swo" | "tmp")
        || name.ends_with('~')
        || name.starts_with('.')
}

/// FileSystem Actor - watches for file changes
pub struct FsActor {
    /// Channel to receive notify events (sync -> async bridge)
    notify_rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    /// Watcher handle (must be kept alive)
    _watcher: RecommendedWatcher,
    /// Channel to send messages to CompilerActor
    compiler_tx: mpsc::Sender<CompilerMsg>,
    /// Debouncer state
    debouncer: Debouncer,
    /// Site configuration for file classification
    config: Arc<SiteConfig>,
    /// Status display for watch mode
    status: WatchStatus,
}

impl FsActor {
    /// Create a new FsActor with Watcher-First pattern
    ///
    /// The watcher starts immediately, buffering events while the caller
    /// performs initial build. This eliminates the "vacuum period".
    pub fn new(
        paths: Vec<PathBuf>,
        compiler_tx: mpsc::Sender<CompilerMsg>,
        config: Arc<SiteConfig>,
    ) -> notify::Result<Self> {
        // 1. Create sync channel for notify (it doesn't support async)
        let (notify_tx, notify_rx) = std::sync::mpsc::channel();

        // 2. Create and configure watcher IMMEDIATELY
        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = notify_tx.send(res);
        })?;

        // 3. Start watching all paths
        for path in &paths {
            watcher.watch(path, RecursiveMode::Recursive)?;
        }

        // Events are now buffering in notify_rx while caller does initial build

        Ok(Self {
            notify_rx,
            _watcher: watcher,
            compiler_tx,
            debouncer: Debouncer::new(),
            config,
            status: WatchStatus::new(),
        })
    }

    /// Run the actor event loop
    pub async fn run(self) {
        // Extract fields before consuming self
        let notify_rx = self.notify_rx;
        let compiler_tx = self.compiler_tx;
        let config = self.config;
        let mut debouncer = self.debouncer;
        let mut status = self.status;

        let (async_tx, mut async_rx) = tokio::sync::mpsc::channel::<notify::Event>(64);

        // Spawn a thread to poll notify events and send to async channel
        std::thread::spawn(move || {
            while let Ok(result) = notify_rx.recv() {
                match result {
                    Ok(event) => {
                        if async_tx.blocking_send(event).is_err() {
                            break; // Receiver dropped
                        }
                    }
                    Err(e) => {
                        crate::log!("watch"; "notify error: {}", e);
                    }
                }
            }
        });

        loop {
            tokio::select! {
                // Receive events from async channel
                Some(event) = async_rx.recv() => {
                    debouncer.add_event(&event);
                }

                // Check debouncer timeout
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if debouncer.ready() {
                        let paths = debouncer.take();
                        if !paths.is_empty() {
                            // Classify files and determine action
                            let result = classify_changes(&paths, &config);

                            // Log changes (separate from classification logic)
                            for (path, category) in &result.classified {
                                status.success(&format!("{} changed: {}", category.name(), path.display()));
                            }

                            // Route to appropriate handler
                            let msg = result_to_message(&result);
                            if let Some(note) = &result.note {
                                status.success(note);
                            }

                            if compiler_tx.send(msg).await.is_err() {
                                // CompilerActor shut down
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Convert ClassifyResult to CompilerMsg
fn result_to_message(result: &ClassifyResult) -> CompilerMsg {
    if result.config_changed {
        CompilerMsg::FullRebuild
    } else if !result.content_to_compile.is_empty() {
        CompilerMsg::Compile(result.content_to_compile.clone())
    } else {
        CompilerMsg::Compile(vec![])
    }
}

// =============================================================================
// Debouncer
// =============================================================================

/// Simple debouncer for file events
struct Debouncer {
    /// Accumulated changed paths
    changed: Vec<PathBuf>,
    /// Time of last event
    last_event: Option<std::time::Instant>,
    /// Time of last compile
    last_compile: Option<std::time::Instant>,
}

impl Debouncer {
    fn new() -> Self {
        Self {
            changed: Vec::new(),
            last_event: None,
            last_compile: None,
        }
    }

    fn add_event(&mut self, event: &notify::Event) {
        self.last_event = Some(std::time::Instant::now());

        for path in &event.paths {
            // Skip editor temporary/backup files
            if is_temp_file(path) {
                continue;
            }

            // Normalize path to ensure consistent keys with VDOM cache
            // Fixes macOS /var vs /private/var symlink issues
            let path = normalize_path(path);

            if !self.changed.contains(&path) {
                self.changed.push(path);
            }
        }
    }

    fn ready(&self) -> bool {
        let Some(last_event) = self.last_event else {
            return false;
        };

        // Must wait for debounce period
        if last_event.elapsed() < Duration::from_millis(DEBOUNCE_MS) {
            return false;
        }

        // Must wait for cooldown from last compile
        if let Some(last_compile) = self.last_compile
            && last_compile.elapsed() < Duration::from_millis(REBUILD_COOLDOWN_MS)
        {
            return false;
        }

        !self.changed.is_empty()
    }

    fn take(&mut self) -> Vec<PathBuf> {
        self.last_event = None;
        self.last_compile = Some(std::time::Instant::now());
        std::mem::take(&mut self.changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debouncer_empty() {
        let debouncer = Debouncer::new();
        assert!(!debouncer.ready());
    }
}
