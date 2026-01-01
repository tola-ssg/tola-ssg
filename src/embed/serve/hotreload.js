// ==========================================================================
// Tola Hot Reload Runtime (Anchor-based)
// ==========================================================================
//
// All operations use StableId (data-tola-id) for targeting.
// No position indices - uses anchor-based insertion instead.
//
// This design ensures:
// - Order independence (operations can execute in any order)
// - No index drift bugs
// - Simple, predictable behavior

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
        console.log('[tola] disconnected, reloading in', this.reconnectDelay, 'ms');
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
      }
    },

    // Apply patch operations
    applyPatches(ops) {
      for (const op of ops) {
        try {
          this.applyPatch(op);
        } catch (err) {
          console.error('[tola] patch failed:', op.op, err);
          location.reload();
          return;
        }
      }
      this.hydrate();
    },

    // Apply single patch - pure ID/anchor based, no position indices
    applyPatch(op) {
      switch (op.op) {
        case 'replace': {
          const el = this.getById(op.target);
          if (el) el.outerHTML = op.html;
          break;
        }

        case 'text': {
          // Update text content (for single-text-child elements)
          const el = this.getById(op.target);
          if (el) {
            el.textContent = op.text;
          } else {
            console.warn('[tola] text target not found:', op.target);
          }
          break;
        }

        case 'html': {
          // Replace inner HTML (for mixed content structure changes)
          const el = this.getById(op.target);
          if (el) el.innerHTML = op.html;
          break;
        }

        case 'remove': {
          const el = this.getById(op.target);
          if (el) {
            el.remove();
            this.idMap.delete(op.target);
          }
          break;
        }

        case 'insert': {
          const anchor = this.getById(op.anchor_id);
          if (!anchor) break;

          switch (op.anchor_type) {
            case 'after':
              anchor.insertAdjacentHTML('afterend', op.html);
              break;
            case 'before':
              anchor.insertAdjacentHTML('beforebegin', op.html);
              break;
            case 'first':
              anchor.insertAdjacentHTML('afterbegin', op.html);
              break;
            case 'last':
              anchor.insertAdjacentHTML('beforeend', op.html);
              break;
          }
          break;
        }

        case 'move': {
          const el = this.getById(op.target);
          const anchor = this.getById(op.anchor_id);
          if (!el || !anchor) break;

          switch (op.anchor_type) {
            case 'after':
              anchor.insertAdjacentElement('afterend', el);
              break;
            case 'before':
              anchor.insertAdjacentElement('beforebegin', el);
              break;
            case 'first':
              anchor.insertAdjacentElement('afterbegin', el);
              break;
            case 'last':
              anchor.insertAdjacentElement('beforeend', el);
              break;
          }
          break;
        }

        case 'attrs': {
          const el = this.getById(op.target);
          if (el) {
            for (const [name, value] of op.attrs) {
              if (value === null) {
                el.removeAttribute(name);
              } else {
                el.setAttribute(name, value);
              }
            }
          }
          break;
        }
      }
    },

    // Get element by StableId
    getById(id) {
      let el = this.idMap.get(id);
      if (el && el.isConnected) return el;

      el = document.querySelector(`[data-tola-id="${id}"]`);
      if (el) this.idMap.set(id, el);
      return el;
    },

    // SyncTeX: get source location from element
    getSourceLocation(el) {
      while (el && el !== document.body) {
        const id = el.dataset?.tolaId;
        if (id) return { id, tag: el.tagName.toLowerCase() };
        el = el.parentElement;
      }
      return null;
    }
  };

  // Initialize
  Tola.connect(__TOLA_WS_PORT__);
  window.Tola = Tola;
})();

