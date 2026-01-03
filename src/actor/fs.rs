//! FileSystem Actor
//!
//! Watches for file changes and sends debounced events to the CompilerActor.
//! Implements the "Watcher-First" pattern for zero event loss.
//!
//! # File Classification
//!
//! Files are categorized to determine the appropriate rebuild strategy:
//! - **Content**: Incremental rebuild of single file
//! - **Deps**: Rebuild all content files that depend on it
//! - **Config**: Full site rebuild
//! - **Asset**: Copy/process single file
//! - **Unknown**: Ignored

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use rustc_hash::FxHashSet;
use tokio::sync::mpsc;

use super::messages::CompilerMsg;
use crate::config::SiteConfig;
use crate::logger::WatchStatus;
use crate::utils::path::normalize_path;

/// Debounce configuration
const DEBOUNCE_MS: u64 = 300;
const REBUILD_COOLDOWN_MS: u64 = 800;

// =============================================================================
// File Category (inline - only used in this module)
// =============================================================================

/// Category of a changed file, determines rebuild strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileCategory {
    /// Content file (.typ) - rebuild individually
    Content,
    /// Asset file - copy individually (currently triggers reload)
    Asset,
    /// Site config (tola.toml) - full rebuild
    Config,
    /// Dependency (templates, utils) - rebuild dependents
    Deps,
    /// Outside watched dirs - ignored
    Unknown,
}

impl FileCategory {
    fn name(self) -> &'static str {
        match self {
            Self::Content => "content",
            Self::Asset => "asset",
            Self::Config => "config",
            Self::Deps => "deps",
            Self::Unknown => "unknown",
        }
    }
}

/// Categorize a path based on config directories.
fn categorize_path(path: &Path, config: &SiteConfig) -> FileCategory {
    let path = normalize_path(path);

    if path == config.config_path {
        FileCategory::Config
    } else if config.build.deps.iter().any(|dep| path.starts_with(dep)) {
        FileCategory::Deps
    } else if path.starts_with(&config.build.content) {
        FileCategory::Content
    } else if path.starts_with(&config.build.assets) {
        FileCategory::Asset
    } else {
        FileCategory::Unknown
    }
}

// =============================================================================
// Dependency Resolution
// =============================================================================

/// Collect all content files that depend on the changed files.
///
/// Uses the global DEPENDENCY_GRAPH to find reverse dependencies.
fn collect_affected_content(changed_files: &[PathBuf]) -> Vec<PathBuf> {
    use crate::compiler::deps::get_dependents;

    let mut affected = FxHashSet::default();

    for path in changed_files {
        affected.extend(get_dependents(path.as_path()));
    }

    affected.into_iter().collect()
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if path is a temp/backup file (editor artifacts).
fn is_temp_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    matches!(ext, "bck" | "bak" | "backup" | "swp" | "swo" | "tmp")
        || name.ends_with('~')
        || name.starts_with('.')
}

/// FileSystem Actor - watches for file changes
pub struct FsActor {
    /// Paths to watch
    paths: Vec<PathBuf>,
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
            paths,
            notify_rx,
            _watcher: watcher,
            compiler_tx,
            debouncer: Debouncer::new(),
            config,
            status: WatchStatus::new(),
        })
    }

    /// Get the watched paths
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
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
                            let msg = result.to_message();
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

// =============================================================================
// Classification Result (pure data, no side effects)
// =============================================================================

/// Result of classifying changed files.
struct ClassifyResult {
    /// Files grouped by category (for logging)
    classified: Vec<(PathBuf, FileCategory)>,
    /// Config changed - requires full rebuild
    config_changed: bool,
    /// Deps that changed
    deps_changed: Vec<PathBuf>,
    /// Content files to compile (direct changes + affected by deps)
    content_to_compile: Vec<PathBuf>,
    /// Optional note (e.g., "deps changed but no dependents")
    note: Option<String>,
}

impl ClassifyResult {
    /// Convert to CompilerMsg
    fn to_message(&self) -> CompilerMsg {
        if self.config_changed {
            CompilerMsg::FullRebuild
        } else if !self.content_to_compile.is_empty() {
            CompilerMsg::Compile(self.content_to_compile.clone())
        } else {
            // No actionable changes
            CompilerMsg::Compile(vec![])
        }
    }
}

/// Classify changed files (pure function, no side effects).
fn classify_changes(paths: &[PathBuf], config: &SiteConfig) -> ClassifyResult {
    let mut classified = Vec::new();
    let mut config_changed = false;
    let mut deps_changed = Vec::new();
    let mut content_changed = Vec::new();

    for path in paths {
        let category = categorize_path(path, config);
        classified.push((path.clone(), category));

        match category {
            FileCategory::Config => config_changed = true,
            FileCategory::Deps => deps_changed.push(path.clone()),
            FileCategory::Content => content_changed.push(path.clone()),
            FileCategory::Asset => content_changed.push(path.clone()),
            FileCategory::Unknown => {}
        }
    }

    // Resolve deps → affected content
    let mut note = None;
    let content_to_compile = if config_changed {
        vec![] // Full rebuild, no need to list files
    } else if !deps_changed.is_empty() {
        let affected = collect_affected_content(&deps_changed);
        if affected.is_empty() {
            note = Some("deps changed but no dependents found".to_string());
            // Will trigger full rebuild via config_changed fallback below
            vec![]
        } else {
            // Merge affected with direct content changes
            affected
                .into_iter()
                .chain(content_changed)
                .collect::<FxHashSet<_>>()
                .into_iter()
                .collect()
        }
    } else {
        content_changed
    };

    // If deps changed but no dependents, treat as config change
    let config_changed = config_changed || (!deps_changed.is_empty() && content_to_compile.is_empty() && note.is_some());

    ClassifyResult {
        classified,
        config_changed,
        deps_changed,
        content_to_compile,
        note,
    }
}

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
        if let Some(last_compile) = self.last_compile {
            if last_compile.elapsed() < Duration::from_millis(REBUILD_COOLDOWN_MS) {
                return false;
            }
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
