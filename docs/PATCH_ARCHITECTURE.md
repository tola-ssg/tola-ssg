# Anchor-based Patch Architecture

## Problem Statement

Current patch system uses **position indices** for insert/remove operations:
- `Insert { parent, position, html }`
- `RemoveAtPosition { parent, position }`

This causes:
1. **Index Drift**: Position N becomes invalid after earlier operations
2. **Order Dependence**: Operations must be sorted carefully
3. **High Coupling**: Both Rust and JS must understand position semantics
4. **Fragility**: Any miscalculation corrupts DOM

## Solution: Anchor-based Operations

Replace position indices with **node references**.

### New Patch Types

```rust
/// Anchor specifies WHERE to insert relative to existing nodes
pub enum InsertAnchor {
    /// Insert after an element with this StableId
    After(StableId),
    /// Insert before an element with this StableId
    Before(StableId),
    /// Insert as first child of parent
    FirstChildOf(StableId),
    /// Insert as last child of parent
    LastChildOf(StableId),
}

pub enum Patch {
    /// Replace element content
    Replace { target: StableId, html: String },

    /// Update text content (parent.textContent = text)
    UpdateText { target: StableId, text: String },

    /// Update attributes
    UpdateAttrs { target: StableId, attrs: Vec<(String, Option<String>)> },

    /// Remove element by ID (not position!)
    Remove { target: StableId },

    /// Insert new content at anchor position
    Insert { anchor: InsertAnchor, html: String },

    /// Move existing element to new anchor position
    Move { target: StableId, anchor: InsertAnchor },
}
```

### Key Properties

1. **Order Independence**: Operations can be executed in any order*
2. **Simple JS**: Just `el.insertAdjacentHTML()` or `el.insertBefore()`
3. **No Index Drift**: Anchors are stable node references
4. **Self-describing**: Each op contains all info needed to execute

*Except: Remove must happen before Insert that would create same ID

## Text Node Handling

Text nodes in DOM don't have `data-tola-id`. Three strategies:

### Strategy A: Single-Text Optimization (Current)

For `<p>Hello</p>` (single text child):
- Use `UpdateText { target: p_id, text: "World" }`
- Parent's `textContent` is updated directly
- No need for text node ID

### Strategy B: Comment Markers

For mixed content `<p>Hello <strong>World</strong></p>`:
```html
<p><!--t:abc-->Hello <!--/t--><strong data-tola-id="xyz">World</strong></p>
```

JS can locate text between markers.

### Strategy C: Sibling Anchoring (Recommended)

Don't give text nodes IDs. Instead, anchor relative to element siblings:

```
Old: [Text("Hello"), <strong>World</strong>]
New: [Text("Hi"),    <strong>World</strong>]
```

Generate: `UpdateText { target: p_id, position: 0, text: "Hi" }`

Wait, this still uses position... Let's think differently:

**For text-only updates within same structure, use UpdateText on parent.**
**For structural changes (add/remove nodes), replace the entire parent.**

This is a pragmatic hybrid:
- Simple cases: Direct text update
- Complex cases: Replace parent subtree

## Recommended Implementation

### Phase 1: Simplify Patch Types

```rust
pub enum Patch {
    // === Content Updates (target by ID) ===
    Replace { target: StableId, html: String },
    UpdateText { target: StableId, text: String },
    UpdateAttrs { target: StableId, attrs: Vec<(String, Option<String>)> },

    // === Structural Changes (anchor-based) ===
    Remove { target: StableId },
    InsertAfter { anchor: StableId, html: String },
    InsertBefore { anchor: StableId, html: String },
    InsertFirst { parent: StableId, html: String },
    InsertLast { parent: StableId, html: String },
    Move { target: StableId, to: MoveAnchor },
}

pub enum MoveAnchor {
    After(StableId),
    Before(StableId),
    FirstChildOf(StableId),
    LastChildOf(StableId),
}
```

### Phase 2: Update Diff Algorithm

When generating Insert:
- Find previous sibling with ID → `InsertAfter`
- Find next sibling with ID → `InsertBefore`
- No siblings with ID → `InsertFirst`/`InsertLast`

### Phase 3: Simplify JS Runtime

```javascript
applyPatch(op) {
    const target = this.getById(op.target);

    switch (op.op) {
        case 'replace':
            target.outerHTML = op.html;
            break;
        case 'text':
            target.textContent = op.text;
            break;
        case 'remove':
            target.remove();
            break;
        case 'insert_after':
            this.getById(op.anchor).insertAdjacentHTML('afterend', op.html);
            break;
        case 'insert_before':
            this.getById(op.anchor).insertAdjacentHTML('beforebegin', op.html);
            break;
        case 'insert_first':
            this.getById(op.parent).insertAdjacentHTML('afterbegin', op.html);
            break;
        case 'insert_last':
            this.getById(op.parent).insertAdjacentHTML('beforeend', op.html);
            break;
        case 'move_after':
            this.getById(op.anchor).insertAdjacentElement('afterend', target);
            break;
        // ... etc
    }
}
```

No position calculations. No index drift. Pure ID lookups.

## Edge Cases

### 1. Insert between text nodes

```
Old: <p><span id="a">A</span></p>
New: <p><span id="a">A</span><span id="b">B</span></p>
```

Solution: `InsertAfter { anchor: "a", html: "<span id='b'>B</span>" }`

### 2. Insert at beginning when first child is text

```
Old: <p>Hello</p>
New: <p><span id="x">X</span>Hello</p>
```

Solution: `InsertFirst { parent: p_id, html: "<span id='x'>X</span>" }`

The text "Hello" naturally shifts. No need to track it.

### 3. All children are text (no anchors available)

```
Old: <p>Hello World</p>
New: <p>Hi World</p>
```

Solution: `UpdateText { target: p_id, text: "Hi World" }` (single text optimization)

Or if structure changed: `Replace { target: p_id, html: "<p ...>...</p>" }`

## Summary

| Old System | New System |
|------------|------------|
| `Insert { position: 2 }` | `InsertAfter { anchor: sibling_id }` |
| `RemoveAtPosition { position: 1 }` | `Remove { target: id }` |
| Position drift bugs | Anchor is always valid |
| Complex JS logic | Simple `insertAdjacent*` calls |
| Order matters | Order mostly independent |

This is a **correctness-first** design. The slight overhead of finding anchors is negligible compared to the robustness gained.
