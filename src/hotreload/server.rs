//! WebSocket Server for Hot Reload
//!
//! Manages WebSocket connections for live reload functionality.
//! Uses a simple broadcast model where all connected clients receive updates.
//!
//! # Thread Safety
//!
//! The server uses `Arc<Mutex<>>` for thread-safe client management.
//! Messages are broadcast from the watcher thread to all connected clients.

use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use tungstenite::protocol::Message;
use tungstenite::{accept, WebSocket};

use super::message::HotReloadMessage;

/// Global broadcast channel for hot reload messages
static BROADCAST: std::sync::LazyLock<Broadcaster> =
    std::sync::LazyLock::new(Broadcaster::new);

/// Broadcast a hot reload message to all connected clients
pub fn broadcast(msg: HotReloadMessage) {
    BROADCAST.send(msg);
}

/// Broadcast a reload message to all clients
pub fn broadcast_reload() {
    broadcast(HotReloadMessage::reload());
}

/// Broadcast a reload with reason
#[allow(dead_code)]
pub fn broadcast_reload_reason(reason: &str) {
    broadcast(HotReloadMessage::reload_with_reason(reason));
}

/// Broadcast VDOM patches to all clients
///
/// Converts Patch operations to JSON-serializable PatchOp and sends them.
pub fn broadcast_patches(path: &str, patches: &[crate::vdom::diff::Patch]) {
    if patches.is_empty() {
        // No patches - page unchanged (should rarely happen)
        return;
    }

    let msg = HotReloadMessage::from_patches(path, patches);
    broadcast(msg);
}

// =============================================================================
// Broadcaster
// =============================================================================

/// Type alias for the WebSocket stream
type WsStream = WebSocket<TcpStream>;

/// Type alias for a shared client connection that can be optionally valid
type SharedClient = Arc<Mutex<Option<WsStream>>>;

/// Thread-safe message broadcaster
struct Broadcaster {
    clients: Mutex<Vec<SharedClient>>,
}

impl Broadcaster {
    fn new() -> Self {
        Self {
            clients: Mutex::new(Vec::new()),
        }
    }

    /// Add a new client connection
    fn add_client(&self, ws: WebSocket<TcpStream>) {
        let client = Arc::new(Mutex::new(Some(ws)));
        self.clients.lock().unwrap().push(client);
    }

    /// Send message to all connected clients
    fn send(&self, msg: HotReloadMessage) {
        let json = msg.to_json();
        let message = Message::Text(json.into());

        let mut clients = self.clients.lock().unwrap();

        // Send to all clients, removing disconnected ones
        clients.retain(|client| {
            let mut guard = client.lock().unwrap();
            if let Some(ws) = guard.as_mut() {
                match ws.send(message.clone()) {
                    Ok(_) => true,
                    Err(_) => {
                        // Connection closed, remove client
                        *guard = None;
                        false
                    }
                }
            } else {
                false
            }
        });
    }

    /// Get number of connected clients
    #[allow(dead_code)]
    fn client_count(&self) -> usize {
        self.clients.lock().unwrap().len()
    }
}

// =============================================================================
// HotReloadServer
// =============================================================================

/// WebSocket server for hot reload
pub struct HotReloadServer {
    port: u16,
    running: Arc<Mutex<bool>>,
}

/// Maximum port retry attempts
const MAX_PORT_RETRIES: u16 = 10;

impl HotReloadServer {
    /// Create a new hot reload server
    pub fn new(port: u16) -> Self {
        Self {
            port,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the WebSocket server in a background thread
    ///
    /// Returns the actual port used (may differ if configured port was in use)
    pub fn start(&self) -> anyhow::Result<u16> {
        let base_port = self.port;
        let running = Arc::clone(&self.running);

        // Try to bind to port with retry mechanism
        let (listener, actual_port) = try_bind_port(base_port, MAX_PORT_RETRIES)?;

        // Set non-blocking for shutdown handling
        listener.set_nonblocking(true)?;

        *running.lock().unwrap() = true;

        // Spawn acceptor thread
        let running_clone = Arc::clone(&running);
        thread::spawn(move || {
            accept_loop(listener, running_clone);
        });

        Ok(actual_port)
    }

    /// Stop the server
    #[allow(dead_code)]
    pub fn stop(&self) {
        *self.running.lock().unwrap() = false;
    }

    /// Check if server is running
    #[allow(dead_code)]
    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
}

/// Try binding to port, retry with incremented port if in use
fn try_bind_port(base_port: u16, max_retries: u16) -> anyhow::Result<(TcpListener, u16)> {
    let mut last_error = None;

    for offset in 0..max_retries {
        let port = base_port.saturating_add(offset);
        match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(listener) => {
                let actual_port = listener.local_addr()?.port();
                return Ok((listener, actual_port));
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to bind WebSocket server after {} attempts: {}",
        max_retries,
        last_error.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Accept loop for incoming WebSocket connections
fn accept_loop(listener: TcpListener, running: Arc<Mutex<bool>>) {
    loop {
        // Check if we should stop
        if !*running.lock().unwrap() {
            break;
        }

        // Try to accept connection
        match listener.accept() {
            Ok((stream, addr)) => {
                crate::log!("hotreload"; "client connected: {}", addr);

                // Set blocking for WebSocket operations
                let _ = stream.set_nonblocking(false);

                // Handle client in separate thread
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No connection available, sleep briefly and retry
                thread::sleep(std::time::Duration::from_millis(100));
                continue;
            }
            Err(e) => {
                crate::log!("hotreload"; "accept error: {}", e);
                thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
}

/// Handle a single WebSocket client connection
fn handle_client(stream: TcpStream) {
    let peer_addr = stream.peer_addr().ok();

    // Perform WebSocket handshake
    let ws = match accept(stream) {
        Ok(ws) => ws,
        Err(e) => {
            if let Some(addr) = peer_addr {
                crate::log!("hotreload"; "handshake failed for {}: {}", addr, e);
            }
            return;
        }
    };

    // Send connected message
    let mut ws = ws;
    let connected_msg = HotReloadMessage::connected().to_json();
    if ws.send(Message::Text(connected_msg.into())).is_err() {
        return;
    }

    // Register client for broadcasts
    BROADCAST.add_client(ws);
}

// =============================================================================
// Client Script
// =============================================================================

use crate::embed::{HOTRELOAD_JS, TemplateVar};

/// Generate and write the hotreload JS file to the output directory.
pub fn generate_hotreload_js(
    output_dir: &std::path::Path,
    ws_port: u16,
) -> std::io::Result<std::path::PathBuf> {
    HOTRELOAD_JS.write_rendered_to(output_dir, &[TemplateVar::WsPort(ws_port)])
}

/// Clean up old hotreload JS files.
pub fn cleanup_old_hotreload_js(output_dir: &std::path::Path, ws_port: u16) -> std::io::Result<()> {
    HOTRELOAD_JS.cleanup_old(output_dir, &[TemplateVar::WsPort(ws_port)])
}

// =============================================================================
// Actor Mode Support
// =============================================================================

/// Start WebSocket server that sends clients to an Actor via channel.
///
/// This is an alternative to `HotReloadServer::start()` for actor-based systems.
/// Instead of using the global `BROADCAST`, clients are sent through the channel.
pub fn start_ws_server_with_channel(
    base_port: u16,
    ws_tx: tokio::sync::mpsc::Sender<crate::actor::messages::WsMsg>,
) -> anyhow::Result<u16> {
    use crate::actor::messages::WsMsg;

    let (listener, actual_port) = try_bind_port(base_port, MAX_PORT_RETRIES)?;
    listener.set_nonblocking(true)?;

    // Spawn acceptor thread
    std::thread::spawn(move || {
        loop {
            match listener.accept() {
                Ok((stream, addr)) => {
                    crate::log!("hotreload"; "client connected: {}", addr);

                    // Set blocking for WebSocket operations
                    let _ = stream.set_nonblocking(false);

                    // Send raw TcpStream to WsActor for handshake
                    let tx = ws_tx.clone();
                    if tx.blocking_send(WsMsg::AddClient(stream)).is_err() {
                        crate::log!("hotreload"; "failed to send client to actor");
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    crate::log!("hotreload"; "accept error: {}", e);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
    });

    Ok(actual_port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hotreload_js_filename() {
        let filename = HOTRELOAD_JS.filename();
        assert!(filename.starts_with(".hotreload-"));
        assert!(filename.ends_with(".js"));
    }
}
