//! WebSocket Actor
//!
//! Manages WebSocket connections and broadcasts patches/reload messages
//! to all connected clients.

use std::net::TcpStream;

use tokio::sync::mpsc;
use tungstenite::protocol::Message;
use tungstenite::WebSocket;

use super::messages::WsMsg;
use crate::hotreload::message::HotReloadMessage;
use crate::vdom::diff::Patch;

/// WebSocket Actor - manages client connections
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
                WsMsg::Patch(patches) => {
                    // Convert patches to JSON message
                    let json = serde_json::json!({
                        "type": "patch",
                        "ops": patches.iter().map(|p| {
                            match p {
                                Patch::Replace { target, html } => {
                                    serde_json::json!({
                                        "type": "replace",
                                        "target": target.to_string(),
                                        "html": html,
                                    })
                                }
                                Patch::UpdateText { target, text } => {
                                    serde_json::json!({
                                        "type": "text",
                                        "target": target.to_string(),
                                        "text": text,
                                    })
                                }
                                Patch::UpdateTextAtPosition { parent, position, text } => {
                                    serde_json::json!({
                                        "type": "textAtPosition",
                                        "parent": parent.to_string(),
                                        "position": position,
                                        "text": text,
                                    })
                                }
                                Patch::Remove { target } => {
                                    serde_json::json!({
                                        "type": "remove",
                                        "target": target.to_string(),
                                    })
                                }
                                Patch::RemoveAtPosition { parent, position } => {
                                    serde_json::json!({
                                        "type": "removeAtPosition",
                                        "parent": parent.to_string(),
                                        "position": position,
                                    })
                                }
                                Patch::Insert { parent, position, html } => {
                                    serde_json::json!({
                                        "type": "insert",
                                        "parent": parent.to_string(),
                                        "position": position,
                                        "html": html,
                                    })
                                }
                                Patch::Move { target, new_parent, position } => {
                                    serde_json::json!({
                                        "type": "move",
                                        "target": target.to_string(),
                                        "newParent": new_parent.to_string(),
                                        "position": position,
                                    })
                                }
                                Patch::UpdateAttrs { target, attrs } => {
                                    serde_json::json!({
                                        "type": "attrs",
                                        "target": target.to_string(),
                                        "attrs": attrs.iter().map(|(k, v)| {
                                            serde_json::json!({ "name": k, "value": v })
                                        }).collect::<Vec<_>>(),
                                    })
                                }
                            }
                        }).collect::<Vec<_>>(),
                    });

                    self.broadcast(Message::Text(json.to_string().into()));
                }

                WsMsg::Reload { reason } => {
                    let msg = HotReloadMessage::reload_with_reason(&reason);
                    self.broadcast(Message::Text(msg.to_json().into()));
                }

                WsMsg::ClientConnected => {
                    crate::log!("ws"; "client connected (total: {})", self.clients.len() + 1);
                    // Client is added externally via add_client()
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

    /// Add a new client connection
    pub fn add_client(&mut self, client: WebSocket<TcpStream>) {
        self.clients.push(client);
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
    fn test_placeholder() {
        // Placeholder test
        assert!(true);
    }
}
