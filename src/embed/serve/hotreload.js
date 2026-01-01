// ==========================================================================
// Tola Hot Reload Runtime
// ==========================================================================
//
// Uses StableId (data-tola-id) for node targeting instead of CSS selectors.
// This enables:
// - Move detection (reordered nodes don't trigger delete+insert)
// - Stable identity across compilations
// - SyncTeX integration (click-to-source navigation)

(function() {
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
        case 'remove_at_pos': {
          // Remove child at a specific position
          // Used for text nodes that don't have their own data-tola-id
          const parent = this.getById(op.parent);
          if (parent) {
            const pos = parseInt(op.position, 10);
            const childNodes = parent.childNodes;
            if (pos < childNodes.length) {
              childNodes[pos].remove();
            }
          }
          break;
        }
        case 'insert': {
          const parent = this.getById(op.parent);
          if (!parent) break;

          // Parse HTML using template element (handles all node types correctly)
          const template = document.createElement('template');
          template.innerHTML = op.html;
          const fragment = template.content;

          // Use childNodes for consistent indexing (includes text nodes)
          // This matches Rust's position calculation which counts all nodes
          const childNodes = parent.childNodes;
          const pos = parseInt(op.position, 10);

          if (pos >= childNodes.length) {
            parent.appendChild(fragment);
          } else {
            // insertBefore works with DocumentFragment and TextNode references
            parent.insertBefore(fragment, childNodes[pos]);
          }
          break;
        }
        case 'move': {
          const target = this.getById(op.target);
          const newParent = this.getById(op.new_parent);
          if (!target || !newParent) break;

          // Use childNodes for consistent indexing (includes text nodes)
          const childNodes = newParent.childNodes;
          const pos = parseInt(op.position, 10);

          // insertBefore automatically removes target from its current position
          // (no need to call target.remove() first)
          if (pos >= childNodes.length) {
            newParent.appendChild(target);
          } else {
            // Don't insert before self (no-op if already at correct position)
            if (childNodes[pos] !== target) {
              newParent.insertBefore(target, childNodes[pos]);
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
