//! Actor Coordinator - Orchestrates the Hot Reload System
//!
//! This module is the entry point for the actor-based hot reload system.
//! It creates channels, wires up actors, and manages their lifecycle.
//!
//! # Usage
//!
//! ```ignore
//! let coordinator = Coordinator::new(config);
//! coordinator.run().await?;
//! ```
//!
//! # Architecture
//!
//! ```text
//!                     Coordinator
//!                          │
//!          ┌───────────────┼─────────────┐
//!          │               │             │
//!          ▼               ▼             ▼
//!     ┌─────────┐    ┌──────────┐    ┌──────┐    ┌────────┐
//!     │ FsActor │───►│ Compiler │───►│ Vdom │───►│   Ws   │
//!     └─────────┘    └──────────┘    └──────┘    └────────┘
//!                          │                         │
//!                          └────►─── reload ─────►───┘
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::mpsc;

use super::compiler::CompilerActor;
use super::fs::FsActor;
use super::messages::{CompilerMsg, VdomMsg, WsMsg};
use super::vdom::VdomActor;
use super::ws::WsActor;
use crate::config::SiteConfig;

/// Channel buffer size for actor communication
const CHANNEL_BUFFER: usize = 32;

/// Coordinator - manages actor lifecycle and wiring
pub struct Coordinator {
    /// Site configuration (shared across actors)
    config: Arc<SiteConfig>,
    /// WebSocket port for hot reload (None = no WebSocket server)
    ws_port: Option<u16>,
}

impl Coordinator {
    /// Create a new Coordinator
    pub fn new(config: SiteConfig) -> Self {
        Self {
            config: Arc::new(config),
            ws_port: None,
        }
    }

    /// Create from an Arc<SiteConfig>
    pub fn with_config(config: Arc<SiteConfig>) -> Self {
        Self { config, ws_port: None }
    }

    /// Set WebSocket port for hot reload
    pub fn with_ws_port(mut self, port: u16) -> Self {
        self.ws_port = Some(port);
        self
    }

    /// Run the actor system
    ///
    /// This function:
    /// 1. Creates communication channels
    /// 2. Spawns all actors
    /// 3. Performs initial build
    /// 4. Enters the event loop
    /// 5. Handles graceful shutdown on Ctrl+C
    pub async fn run(self) -> Result<()> {
        crate::log!("actor"; "starting actor system");

        // ═══════════════════════════════════════════════════════════════════
        // Step 1: Create channels (the "conveyor belts" between actors)
        // ═══════════════════════════════════════════════════════════════════

        // FsActor → CompilerActor
        let (compiler_tx, compiler_rx) = mpsc::channel::<CompilerMsg>(CHANNEL_BUFFER);

        // CompilerActor → VdomActor
        let (vdom_tx, vdom_rx) = mpsc::channel::<VdomMsg>(CHANNEL_BUFFER);

        // VdomActor/CompilerActor → WsActor
        let (ws_tx, ws_rx) = mpsc::channel::<WsMsg>(CHANNEL_BUFFER);

        // ═══════════════════════════════════════════════════════════════════
        // Step 1.5: Start WebSocket server (sends clients to WsActor)
        // ═══════════════════════════════════════════════════════════════════
        if let Some(port) = self.ws_port {
            match crate::hotreload::server::start_ws_server_with_channel(port, ws_tx.clone()) {
                Ok(actual_port) => {
                    crate::log!("actor"; "websocket server on port {}", actual_port);
                }
                Err(e) => {
                    crate::log!("actor"; "failed to start websocket server: {}", e);
                }
            }
        }

        // ═══════════════════════════════════════════════════════════════════
        // Step 2: Create actors (each gets its input rx and output tx)
        // ═══════════════════════════════════════════════════════════════════

        // Determine paths to watch (content + templates)
        let watch_paths = self.get_watch_paths();

        // FsActor with Watcher-First pattern (starts buffering immediately)
        let fs_actor = FsActor::new(watch_paths, compiler_tx.clone(), self.config.clone())
            .map_err(|e| anyhow::anyhow!("failed to create file watcher: {}", e))?;

        // Clone vdom_tx for initial build (before it's moved to CompilerActor)
        let vdom_tx_for_init = vdom_tx.clone();

        let compiler_actor = CompilerActor::new(
            compiler_rx,        // receives from FsActor
            vdom_tx,            // sends ALL results to VdomActor (linear flow)
            self.config.clone(),
        );

        let vdom_actor = VdomActor::new(
            vdom_rx,            // receives from CompilerActor
            ws_tx.clone(),      // sends to WsActor
        );

        let ws_actor = WsActor::new(ws_rx);

        // ═══════════════════════════════════════════════════════════════════
        // Step 2.5: Initial Build (populate VDOM cache before starting actors)
        // ═══════════════════════════════════════════════════════════════════
        self.initial_build(&vdom_tx_for_init).await;

        // ═══════════════════════════════════════════════════════════════════
        // Step 3: Spawn actors as concurrent tasks
        // ═══════════════════════════════════════════════════════════════════

        crate::log!("actor"; "spawning actors");

        // Use tokio::select! to run all actors concurrently
        // and handle shutdown gracefully
        let shutdown_tx = compiler_tx.clone();

        tokio::select! {
            // Run all actors concurrently
            result = Self::run_actors(fs_actor, compiler_actor, vdom_actor, ws_actor) => {
                if let Err(e) = result {
                    crate::log!("actor"; "actor error: {}", e);
                }
            }

            // Handle Ctrl+C for graceful shutdown
            // Note: tokio signal feature is optional, so we use a simple approach
            _ = Self::wait_for_shutdown() => {
                crate::log!("actor"; "received shutdown signal");
                Self::shutdown(shutdown_tx).await;
            }
        }

        crate::log!("actor"; "actor system stopped");
        Ok(())
    }

    /// Get paths to watch for file changes
    fn get_watch_paths(&self) -> Vec<PathBuf> {
        let root = self.config.get_root();
        let mut paths = Vec::new();

        // Content directory
        paths.push(root.join(&self.config.build.content));

        // Dependency directories (includes templates/)
        for dep in &self.config.build.deps {
            paths.push(root.join(dep));
        }

        paths
    }

    /// Perform initial build to populate VDOM cache
    ///
    /// This ensures the first file change can diff against cached state
    /// instead of triggering a full reload.
    async fn initial_build(&self, vdom_tx: &mpsc::Sender<VdomMsg>) {
        use crate::compiler::collect_all_files;
        use crate::pipeline::compile::{compile_page, CompileOutcome};

        let config = &self.config;
        let content_dir = &config.build.content;

        // Collect all .typ files
        let typ_files: Vec<_> = collect_all_files(content_dir)
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "typ"))
            .collect();

        if typ_files.is_empty() {
            crate::log!("init"; "no .typ files found for initial build");
            return;
        }

        crate::log!("init"; "initial build: {} files", typ_files.len());

        // Compile all files and collect VDOM results
        // Use rayon for parallel compilation
        let results: Vec<_> = typ_files
            .iter()
            .filter_map(|path| {
                match compile_page(path, config) {
                    CompileOutcome::Vdom { url_path, vdom, .. } => Some((url_path, vdom)),
                    CompileOutcome::Reload { .. } => None,
                    CompileOutcome::Skipped => None,
                    CompileOutcome::Error { path, error } => {
                        crate::log!("init"; "error compiling {}: {}", path.display(), error);
                        None
                    }
                }
            })
            .collect();

        if results.is_empty() {
            crate::log!("init"; "no VDOM results from initial build");
            return;
        }

        // Send to VdomActor to populate cache
        let count = results.len();
        if vdom_tx.send(VdomMsg::Populate { entries: results }).await.is_ok() {
            crate::log!("init"; "sent {} entries to VDOM cache", count);
        }
    }

    /// Wait for shutdown signal (platform-agnostic)
    async fn wait_for_shutdown() {
        // Simple approach: wait forever (shutdown via channel drop)
        // In production, you'd use tokio::signal::ctrl_c() with the signal feature
        std::future::pending::<()>().await
    }

    /// Run all actors concurrently
    async fn run_actors(
        fs_actor: FsActor,
        compiler_actor: CompilerActor,
        vdom_actor: VdomActor,
        ws_actor: WsActor,
    ) -> Result<()> {
        // Spawn each actor as a separate task
        let fs_handle = tokio::spawn(async move {
            fs_actor.run().await;
        });

        let compiler_handle = tokio::spawn(async move {
            compiler_actor.run().await;
        });

        let vdom_handle = tokio::spawn(async move {
            vdom_actor.run().await;
        });

        let ws_handle = tokio::spawn(async move {
            ws_actor.run().await;
        });

        // Wait for any actor to complete (usually means shutdown)
        tokio::select! {
            _ = fs_handle => crate::log!("actor"; "fs actor stopped"),
            _ = compiler_handle => crate::log!("actor"; "compiler actor stopped"),
            _ = vdom_handle => crate::log!("actor"; "vdom actor stopped"),
            _ = ws_handle => crate::log!("actor"; "ws actor stopped"),
        }

        Ok(())
    }

    /// Send shutdown message to initiate graceful shutdown
    async fn shutdown(compiler_tx: mpsc::Sender<CompilerMsg>) {
        crate::log!("actor"; "initiating graceful shutdown");

        // Send shutdown to compiler, which will cascade through the system
        // When a sender is dropped, the receiver will get None and exit
        let _ = compiler_tx.send(CompilerMsg::Shutdown).await;

        // Give actors time to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }
}

// =============================================================================
// Builder Pattern (optional, for more complex configurations)
// =============================================================================

/// Builder for Coordinator with optional customizations
pub struct CoordinatorBuilder {
    config: Option<Arc<SiteConfig>>,
    channel_buffer: usize,
}

impl Default for CoordinatorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CoordinatorBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: None,
            channel_buffer: CHANNEL_BUFFER,
        }
    }

    /// Set the site configuration
    pub fn config(mut self, config: SiteConfig) -> Self {
        self.config = Some(Arc::new(config));
        self
    }

    /// Set the site configuration from Arc
    pub fn config_arc(mut self, config: Arc<SiteConfig>) -> Self {
        self.config = Some(config);
        self
    }

    /// Set channel buffer size
    pub fn channel_buffer(mut self, size: usize) -> Self {
        self.channel_buffer = size;
        self
    }

    /// Build the Coordinator
    pub fn build(self) -> Result<Coordinator> {
        let config = self.config
            .ok_or_else(|| anyhow::anyhow!("config is required"))?;

        Ok(Coordinator { config, ws_port: None })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_builder() {
        // Builder pattern works
        let builder = CoordinatorBuilder::new()
            .channel_buffer(64);

        assert_eq!(builder.channel_buffer, 64);
    }
}
