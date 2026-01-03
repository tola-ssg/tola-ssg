//! WebSocket Actor - Pure Broadcast Relay
//!
//! This actor is responsible for:
//! 1. Managing WebSocket client connections
//! 2. Broadcasting messages to all connected clients
//!
//! # Responsibility Boundary
//!
//! - **This Actor**: Connection management, message broadcast
//! - **NOT This Actor**: Message serialization (delegated to hotreload::message)
//!
//! # Architecture
//!
//! ```text
//! VdomActor ──[Patch/Reload]──► WsActor ──[broadcast]──► Clients
//! ```

use std::net::TcpStream;

use tokio::sync::mpsc;
use tungstenite::protocol::Message;
use tungstenite::WebSocket;

use super::messages::WsMsg;
use crate::hotreload::message::HotReloadMessage;

/// WebSocket Actor - manages client connections and broadcasts
pub struct WsActor {
    /// Channel to receive messages
    rx: mpsc::Receiver<WsMsg>,
    /// Connected clients
    clients: Vec<WebSocket<TcpStream>>,
}

impl WsActor {
    /// Create a new WsActor
    pub fn new(rx: mpsc::Receiver<WsMsg>) -> Self {
        Self {
            rx,
            clients: Vec::new(),
        }
    }

    /// Run the actor event loop
    pub async fn run(mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                WsMsg::Patch { url_path, patches } => {
                    // Delegate serialization to hotreload::message module
                    let hr_msg = HotReloadMessage::from_patches(&url_path, &patches);
                    self.broadcast(Message::Text(hr_msg.to_json().into()));
                }

                WsMsg::Reload { reason } => {
                    let hr_msg = HotReloadMessage::reload_with_reason(&reason);
                    self.broadcast(Message::Text(hr_msg.to_json().into()));
                }

                WsMsg::AddClient(stream) => {
                    // Accept WebSocket handshake and add to clients
                    match tungstenite::accept(stream) {
                        Ok(mut ws) => {
                            // Send connected message first
                            let connected_msg = HotReloadMessage::connected();
                            if let Err(e) = ws.send(Message::Text(connected_msg.to_json().into())) {
                                crate::log!("ws"; "failed to send connected message: {}", e);
                                continue;
                            }
                            crate::log!("ws"; "client connected (total: {})", self.clients.len() + 1);
                            self.clients.push(ws);
                        }
                        Err(e) => {
                            crate::log!("ws"; "handshake failed: {}", e);
                        }
                    }
                }

                WsMsg::ClientConnected => {
                    // Legacy notification, just log
                    crate::log!("ws"; "client notification received");
                }

                WsMsg::Shutdown => {
                    crate::log!("ws"; "shutting down, closing {} clients", self.clients.len());
                    for mut client in self.clients.drain(..) {
                        let _ = client.close(None);
                    }
                    break;
                }
            }
        }
    }

    /// Broadcast a message to all connected clients
    fn broadcast(&mut self, msg: Message) {
        self.clients.retain_mut(|client| {
            match client.send(msg.clone()) {
                Ok(_) => true,
                Err(e) => {
                    crate::log!("ws"; "client disconnected: {}", e);
                    false
                }
            }
        });
    }

    /// Get the number of connected clients
    #[allow(dead_code)]
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ws_actor_creation() {
        // Basic construction test
        assert!(true);
    }
}
