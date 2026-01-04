# tola-vdom

Type-safe multi-phase HTML/XML DOM with TTG (Trees That Grow) pattern.

## Overview

A virtual DOM library designed for compile-time phase safety. Uses GATs (Generic Associated Types) to ensure DOM nodes carry phase-appropriate data, preventing invalid state transitions at compile time.

## Architecture

### Phase System

Documents progress through well-defined phases:

```
Raw -> Indexed -> Processed -> Rendered
```

Each phase adds computed metadata:

| Phase | Description |
|-------|-------------|
| `Raw` | Parsed from source, no computed data |
| `Indexed` | StableIds assigned, tag families identified |
| `Processed` | Transforms applied, ready for rendering |
| `Rendered` | Final output produced |

### Tag Families

Elements are classified into families for specialized processing:

| Family | Tags | Purpose |
|--------|------|---------|
| `Svg` | `svg` | Vector graphics with content hash |
| `Link` | `a` | Internal/external link detection |
| `Heading` | `h1`-`h6` | Table of contents generation |
| `Media` | `img`, `video`, `audio` | Asset processing |
| `Other` | Everything else | Pass-through |

### Transform Pipeline

```rust
use tola_vdom::{Document, Raw, Transform, Processor};
use tola_vdom::transforms::Indexer;

let raw: Document<Raw> = parse_html(source);
let indexed = Indexer::new().transform(raw);
let processed = Processor::new().transform(indexed);
```

## Features

- `std` (default): Standard library support
- `serde`: Serialization support
- `parallel`: Parallel processing with rayon
- `hotreload`: WebSocket-based hot reload support
- `rkyv`: Zero-copy serialization

## Modules

| Module | Description |
|--------|-------------|
| `phase` | Phase trait and type definitions |
| `node` | Document, Element, Text, Node types |
| `family` | TagFamily trait and implementations |
| `attr` | Attribute storage |
| `transform` | Transform trait and Pipeline |
| `transforms` | Indexer, Processor, HtmlRenderer |
| `diff` | VDOM diff algorithm |
| `id` | Content-hash based StableId |
| `convert` | Typst HTML to Raw VDOM conversion |

## Usage

### Basic Pipeline

```rust
use tola_vdom::{compile_to_html, from_typst_html};

// From typst-html document
let result = compile_to_html(&typst_document);
std::fs::write("output.html", &result.html)?;
```

### Custom Transforms

```rust
use tola_vdom::{Transform, Document, Indexed, Processed};

struct MyTransform;

impl Transform<Document<Indexed>> for MyTransform {
    type Output = Document<Processed>;

    fn transform(&self, doc: Document<Indexed>) -> Self::Output {
        // Custom transformation logic
    }
}
```

### Diffing

```rust
use tola_vdom::{diff, Patch};

let patches = diff(&old_doc, &new_doc);
for patch in patches {
    match patch {
        Patch::Insert { .. } => { /* handle */ }
        Patch::Remove { .. } => { /* handle */ }
        Patch::Replace { .. } => { /* handle */ }
        Patch::UpdateAttrs { .. } => { /* handle */ }
    }
}
```

## Requirements

- Rust 1.85+ (edition 2024)
- Typst 0.14.1 (for convert module)

## License

MIT
