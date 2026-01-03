//! WebSocket Server for Hot Reload (Actor Mode)
//!
//! Provides WebSocket server that integrates with the Actor system.
//! Clients are sent to WsActor via channel for message handling.

use std::net::TcpListener;

use crate::actor::messages::WsMsg;
use crate::embed::{HOTRELOAD_JS, TemplateVar};

/// Maximum port retry attempts
const MAX_PORT_RETRIES: u16 = 10;

// =============================================================================
// Actor Mode WebSocket Server
// =============================================================================

/// Start WebSocket server that sends clients to WsActor via channel.
///
/// This is the primary API for actor-based hot reload.
/// Clients are sent through the channel for WsActor to handle.
pub fn start_ws_server_with_channel(
    base_port: u16,
    ws_tx: tokio::sync::mpsc::Sender<WsMsg>,
) -> anyhow::Result<u16> {
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

// =============================================================================
// Helpers
// =============================================================================

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

// =============================================================================
// Client Script
// =============================================================================

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
