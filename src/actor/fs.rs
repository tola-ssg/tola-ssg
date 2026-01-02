//! FileSystem Actor
//!
//! Watches for file changes and sends debounced events to the CompilerActor.
//! Implements the "Watcher-First" pattern for zero event loss.

use std::path::PathBuf;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use super::messages::{CompilerMsg, FsMsg};

/// Debounce configuration
const DEBOUNCE_MS: u64 = 300;
const REBUILD_COOLDOWN_MS: u64 = 800;

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
}

impl FsActor {
    /// Create a new FsActor with Watcher-First pattern
    ///
    /// The watcher starts immediately, buffering events while the caller
    /// performs initial build. This eliminates the "vacuum period".
    pub fn new(
        paths: Vec<PathBuf>,
        compiler_tx: mpsc::Sender<CompilerMsg>,
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
        })
    }

    /// Get the watched paths
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        loop {
            tokio::select! {
                // Check for notify events (via spawn_blocking since it's sync)
                result = tokio::task::spawn_blocking({
                    let rx = self.notify_rx.try_recv();
                    move || rx
                }) => {
                    match result {
                        Ok(Ok(Ok(event))) => {
                            // Add to debouncer
                            self.debouncer.add_event(&event);
                        }
                        Ok(Ok(Err(e))) => {
                            crate::log!("watch"; "notify error: {}", e);
                        }
                        Ok(Err(std::sync::mpsc::TryRecvError::Empty)) => {
                            // No events, continue
                        }
                        Ok(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                            crate::log!("watch"; "watcher disconnected");
                            break;
                        }
                        Err(e) => {
                            crate::log!("watch"; "spawn_blocking error: {}", e);
                        }
                    }
                }

                // Check debouncer timeout
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if self.debouncer.ready() {
                        let paths = self.debouncer.take();
                        if !paths.is_empty() {
                            if self.compiler_tx.send(CompilerMsg::Compile(paths)).await.is_err() {
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
            if !self.changed.contains(path) {
                self.changed.push(path.clone());
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
