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
/// Converts StableIdPatch operations to JSON-serializable PatchOp and sends them.
/// Falls back to full reload if patch list is empty.
pub fn broadcast_patches(path: &str, patches: &[super::diff::StableIdPatch]) {
    if patches.is_empty() {
        // No patches - page unchanged (should rarely happen)
        return;
    }

    let msg = HotReloadMessage::from_stable_id_patches(path, patches);
    broadcast(msg);
}

// =============================================================================
// Broadcaster
// =============================================================================

/// Thread-safe message broadcaster
struct Broadcaster {
    clients: Mutex<Vec<Arc<Mutex<Option<WebSocket<TcpStream>>>>>>,
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

/// Get the JavaScript client code for hot reload
///
/// This returns a small JS snippet that connects to the WebSocket server
/// and handles reload messages.
///
/// # StableId-based Patching
///
/// The runtime uses `data-tola-id` attributes for node targeting:
/// - Elements are identified by their StableId (derived from Typst Span)
/// - This enables accurate patching even when DOM structure changes
/// - Move operations preserve element state (scroll position, focus, etc.)
pub fn get_client_script(ws_port: u16) -> String {
    let s = r#"<script>
(function() {
  // ==========================================================================
  // Tola Hot Reload Runtime
  // ==========================================================================
  //
  // Uses StableId (data-tola-id) for node targeting instead of CSS selectors.
  // This enables:
  // - Move detection (reordered nodes don't trigger delete+insert)
  // - Stable identity across compilations
  // - SyncTeX integration (click-to-source navigation)

  const Tola = {
    // StableId -> Element mapping for O(1) lookups
    idMap: new Map(),
    ws: null,
    reconnectDelay: 1000,

    // Hydrate: build idMap from existing DOM
    hydrate() {
      this.idMap.clear();
      document.querySelectorAll('[data-tola-id]').forEach(el => {
        this.idMap.set(el.dataset.tolaId, el);
      });
      console.log('[tola] hydrated', this.idMap.size, 'nodes');
    },

    // Connect to WebSocket server
    connect(port) {
      this.ws = new WebSocket(`ws://localhost:${port}/`);

      this.ws.onopen = () => {
        console.log('[tola] hot reload connected');
        this.reconnectDelay = 1000;
        this.hydrate();
      };

      this.ws.onmessage = (e) => {
        try {
          const msg = JSON.parse(e.data);
          this.handleMessage(msg);
        } catch (err) {
          console.error('[tola] message error:', err);
        }
      };

      this.ws.onclose = () => {
        console.log('[tola] connection closed, reconnecting in', this.reconnectDelay, 'ms');
        setTimeout(() => {
          this.reconnectDelay = Math.min(this.reconnectDelay * 1.5, 10000);
          location.reload();
        }, this.reconnectDelay);
      };

      this.ws.onerror = (err) => {
        console.error('[tola] WebSocket error:', err);
      };
    },

    // Handle incoming message
    handleMessage(msg) {
      switch (msg.type) {
        case 'reload':
          console.log('[tola] reloading:', msg.reason || 'file changed');
          location.reload();
          break;
        case 'patch':
          this.applyPatches(msg.ops);
          break;
        case 'connected':
          console.log('[tola] server version:', msg.version);
          break;
        case 'full_sync':
          // Full document replacement
          document.documentElement.innerHTML = msg.html;
          this.hydrate();
          break;
      }
    },

    // Apply patch operations
    applyPatches(ops) {
      for (const op of ops) {
        try {
          this.applyPatch(op);
        } catch (err) {
          console.error('[tola] patch error:', op, err);
          // Fallback to full reload on error
          location.reload();
          return;
        }
      }
      // Re-hydrate after patches to update idMap
      this.hydrate();
    },

    // Apply single patch operation
    applyPatch(op) {
      switch (op.op) {
        case 'replace': {
          const target = this.getById(op.target);
          if (target) {
            target.outerHTML = op.html;
          }
          break;
        }
        case 'text': {
          const target = this.getById(op.target);
          if (target) {
            target.textContent = op.text;
          }
          break;
        }
        case 'text_at_pos': {
          // Update text content at a specific child position
          // Used for text nodes that don't have their own data-tola-id
          const parent = this.getById(op.parent);
          if (parent) {
            const pos = parseInt(op.position, 10);
            const childNodes = parent.childNodes;
            if (pos < childNodes.length) {
              const node = childNodes[pos];
              if (node.nodeType === Node.TEXT_NODE) {
                node.textContent = op.text;
              } else if (node.nodeType === Node.ELEMENT_NODE) {
                // If it's an element, set its textContent
                node.textContent = op.text;
              }
            } else {
              // Position out of bounds - append as new text node
              parent.appendChild(document.createTextNode(op.text));
            }
          }
          break;
        }
        case 'remove': {
          const target = this.getById(op.target);
          if (target) {
            target.remove();
            this.idMap.delete(op.target);
          }
          break;
        }
        case 'insert': {
          const parent = this.getById(op.parent);
          if (parent) {
            // Defensive insert: avoid duplicating elements that already exist
            const temp = document.createElement('div');
            temp.innerHTML = op.html;
            const newIds = Array.from(temp.querySelectorAll('[data-tola-id]')).map(el => el.dataset.tolaId);

            // If any of the new IDs already exist in the document, perform targeted replaces
            if (newIds.some(id => document.querySelector(`[data-tola-id="${id}"]`))) {
              newIds.forEach(id => {
                const newEl = temp.querySelector(`[data-tola-id="${id}"]`);
                const existing = document.querySelector(`[data-tola-id="${id}"]`);
                if (newEl && existing) {
                  existing.outerHTML = newEl.outerHTML;
                }
              });
            } else {
              // Insert at specific position
              const children = parent.children;
              if (op.position >= children.length) {
                parent.insertAdjacentHTML('beforeend', op.html);
              } else {
                children[op.position].insertAdjacentHTML('beforebegin', op.html);
              }
            }
          }
          break;
        }
        case 'move': {
          const target = this.getById(op.target);
          const newParent = this.getById(op.new_parent);
          if (target && newParent) {
            // Remove from current position
            target.remove();
            // Insert at new position
            const children = newParent.children;
            if (op.position >= children.length) {
              newParent.appendChild(target);
            } else {
              newParent.insertBefore(target, children[op.position]);
            }
          }
          break;
        }
        case 'attrs': {
          const target = this.getById(op.target);
          if (target) {
            for (const [name, value] of op.attrs) {
              if (value === null) {
                target.removeAttribute(name);
              } else {
                target.setAttribute(name, value);
              }
            }
          }
          break;
        }
        // Legacy CSS selector-based ops (backward compatibility)
        default: {
          const target = document.querySelector(op.target);
          if (target) {
            if (op.op === 'replace') target.outerHTML = op.html;
            else if (op.op === 'text') target.textContent = op.text;
            else if (op.op === 'remove') target.remove();
          }
        }
      }
    },

    // Get element by StableId
    getById(id) {
      // Try cache first
      let el = this.idMap.get(id);
      if (el && el.isConnected) return el;

      // Fallback to querySelector
      el = document.querySelector(`[data-tola-id="${id}"]`);
      if (el) {
        this.idMap.set(id, el);
      }
      return el;
    },

    // SyncTeX: get source location from element
    getSourceLocation(el) {
      while (el && el !== document.body) {
        const id = el.dataset?.tolaId;
        if (id) {
          return { id, tag: el.tagName.toLowerCase() };
        }
        el = el.parentElement;
      }
      return null;
    }
  };

  // Initialize
  Tola.connect(__TOLA_WS_PORT__);
  window.Tola = Tola;
})();
</script>"#;

    s.replace("__TOLA_WS_PORT__", &ws_port.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_script_generation() {
        let script = get_client_script(35729);
        // Basic sanity checks
        assert!(!script.is_empty(), "Script should not be empty");
        assert!(script.starts_with("<script>"), "Script should start with <script> tag");
        assert!(script.ends_with("</script>"), "Script should end with </script> tag");
        // Check key components
        assert!(script.contains("35729"), "Script should contain the port number");
        assert!(script.contains("ws://"), "Script should contain WebSocket URL");
        assert!(script.contains("Tola"), "Script should contain Tola object");
    }
}
