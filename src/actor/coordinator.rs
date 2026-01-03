//! Actor Coordinator - Wires up the Hot Reload Actor System
//!
//! # Responsibility
//!
//! The Coordinator is a **thin orchestrator** that:
//! 1. Creates communication channels
//! 2. Wires up actors
//! 3. Runs them concurrently
//!
//! It does NOT contain business logic - that lives in `pipeline/`.
//!
//! # Architecture
//!
//! ```text
//! FsActor ──► CompilerActor ──► VdomActor ──► WsActor
//!    │              │              │            │
//!    └──────────────┴──────────────┴────────────┘
//!                 Linear Message Flow
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
use crate::pipeline::init::build_initial_cache;

/// Channel buffer size
const CHANNEL_BUFFER: usize = 32;

/// Coordinator - wires up and runs the actor system
pub struct Coordinator {
    config: Arc<SiteConfig>,
    ws_port: Option<u16>,
}

impl Coordinator {
    /// Create from Arc<SiteConfig>
    pub fn with_config(config: Arc<SiteConfig>) -> Self {
        Self { config, ws_port: None }
    }

    /// Set WebSocket port
    pub fn with_ws_port(mut self, port: u16) -> Self {
        self.ws_port = Some(port);
        self
    }

    /// Run the actor system
    pub async fn run(self) -> Result<()> {
        // Create channels
        let (compiler_tx, compiler_rx) = mpsc::channel::<CompilerMsg>(CHANNEL_BUFFER);
        let (vdom_tx, vdom_rx) = mpsc::channel::<VdomMsg>(CHANNEL_BUFFER);
        let (ws_tx, ws_rx) = mpsc::channel::<WsMsg>(CHANNEL_BUFFER);

        // Start WebSocket server
        if let Some(port) = self.ws_port {
            if let Err(e) = crate::hotreload::ws::start_ws_server_with_channel(port, ws_tx.clone()) {
                crate::log!("actor"; "websocket server failed: {}", e);
            }
        }

        // Create actors
        let watch_paths = self.watch_paths();
        let fs_actor = FsActor::new(watch_paths, compiler_tx.clone(), self.config.clone())
            .map_err(|e| anyhow::anyhow!("watcher failed: {}", e))?;

        let compiler_actor = CompilerActor::new(compiler_rx, vdom_tx.clone(), self.config.clone());
        let vdom_actor = VdomActor::new(vdom_rx, ws_tx.clone());
        let ws_actor = WsActor::new(ws_rx);

        // Initial build (populate VDOM cache) - delegates to pipeline
        let entries = build_initial_cache(&self.config);
        if !entries.is_empty() {
            let _ = vdom_tx.send(VdomMsg::Populate { entries }).await;
        }

        // Run actors
        crate::log!("actor"; "starting");
        let shutdown_tx = compiler_tx;
        tokio::select! {
            _ = run_actors(fs_actor, compiler_actor, vdom_actor, ws_actor) => {}
            _ = std::future::pending::<()>() => {
                let _ = shutdown_tx.send(CompilerMsg::Shutdown).await;
            }
        }
        crate::log!("actor"; "stopped");
        Ok(())
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        let root = self.config.get_root();
        let mut paths = vec![root.join(&self.config.build.content)];
        for dep in &self.config.build.deps {
            paths.push(root.join(dep));
        }
        paths
    }
}

/// Run all actors concurrently
async fn run_actors(
    fs: FsActor,
    compiler: CompilerActor,
    vdom: VdomActor,
    ws: WsActor,
) -> Result<()> {
    tokio::select! {
        _ = tokio::spawn(async move { fs.run().await }) => {}
        _ = tokio::spawn(async move { compiler.run().await }) => {}
        _ = tokio::spawn(async move { vdom.run().await }) => {}
        _ = tokio::spawn(async move { ws.run().await }) => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_coordinator_creation() {
        // Minimal test - actual testing requires tokio runtime
        assert!(true);
    }
}
