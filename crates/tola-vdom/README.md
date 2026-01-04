# tola-vdom

Type-safe multi-phase HTML/XML DOM with TTG (Trees That Grow) pattern.

## Overview

A virtual DOM library designed for **compile-time safety**. Uses GATs (Generic Associated Types) to ensure:

1. **Phase Safety**: DOM nodes carry phase-appropriate data, preventing invalid state transitions
2. **Capability Safety**: Transform dependencies are checked at compile time via the capability system

## Architecture

### Three-tier State Model

```
┌─────────────────────────────────────────────────────────────────┐
│  Level 1: Phase (Memory Layout)                                 │
│  Raw → Indexed → Processed → Rendered                           │
├─────────────────────────────────────────────────────────────────┤
│  Level 2: Capability (Processing Progress - Zero Overhead)      │
│  LinksChecked, SvgOptimized, HeadingsProcessed, ...             │
├─────────────────────────────────────────────────────────────────┤
│  Level 3: Family State (Element-level Enum)                     │
│  Link::Pending → Link::Resolved, Svg::Raw → Svg::Optimized      │
└─────────────────────────────────────────────────────────────────┘
```

### Phase System

Documents progress through well-defined phases:

| Phase | Description |
|-------|-------------|
| `Raw` | Parsed from source, no computed data |
| `Indexed` | StableIds assigned, tag families identified |
| `Processed` | Transforms applied, ready for rendering |
| `Rendered` | Final output produced |

### Capability System

**Zero-cost compile-time dependency checking.** Wrong pipeline order = compile error.

```rust
use tola_vdom::capability::*;

// Built-in capability markers (zero-sized types, no runtime cost)
// LinksCheckedCap, LinksResolvedCap, SvgOptimizedCap,
// HeadingsProcessedCap, MediaProcessedCap, MetadataExtractedCap

// ─────────────────────────────────────────────────────────────────────────────
// #[requires] macro: declare what capabilities a function needs
// ─────────────────────────────────────────────────────────────────────────────

#[requires(C: LinksCheckedCap)]              // "I need links to be checked first"
fn resolve_links<C>(doc: Doc<Indexed, C>) {
    // Compiler guarantees LinksCheckedCap is present - safe to resolve!
}

#[requires(C: LinksCheckedCap, SvgOptimizedCap)]   // Multiple requirements
fn final_render<C>(doc: Doc<Indexed, C>) {
    // Both capabilities guaranteed present
}

// ─────────────────────────────────────────────────────────────────────────────
// Pipeline: capabilities accumulate as transforms run
// ─────────────────────────────────────────────────────────────────────────────

let doc = Doc::new(indexed_doc);             // EmptyCap
let doc = check_links(doc);                  // caps![LinksCheckedCap]
let doc = optimize_svg(doc);                 // caps![SvgOptimizedCap, LinksCheckedCap]
let doc = resolve_links(doc);                // ✓ OK: LinksCheckedCap present

// ─────────────────────────────────────────────────────────────────────────────
// Wrong order? Compile error!
// ─────────────────────────────────────────────────────────────────────────────

let doc = Doc::new(indexed_doc);             // EmptyCap
let doc = resolve_links(doc);                // ✗ ERROR!
//        ^^^^^^^^^^^^^ capability `LinksCheckedCap` is required but not available
//        note: try adding the appropriate Transform earlier in the pipeline
```

### Tag Families

Elements are classified into families for specialized processing:

| Family | Tags | Purpose |
|--------|------|---------|
| `Svg` | `svg` | Vector graphics with content hash |
| `Link` | `a` | Internal/external link detection |
| `Heading` | `h1`-`h6` | Table of contents generation |
| `Media` | `img`, `video`, `audio` | Asset processing |
| `Other` | Everything else | Pass-through |

## Features

- `typst` (default): Typst HTML document conversion
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
| `capability` | Compile-time capability system |
| `attr` | Attribute storage |
| `transform` | Transform trait and Pipeline |
| `diff` | VDOM diff algorithm |
| `lcs` | Longest Common Subsequence algorithm |
| `id` | Content-hash based StableId |
| `cache` | VDOM caching utilities |
| `convert` | Typst HTML to Raw VDOM conversion |

## Usage

### Basic Pipeline

```rust
use tola_vdom::{Document, Raw, Transform, Processor};
use tola_vdom::transform::Indexer;

let raw: Document<Raw> = parse_html(source);
let indexed = Indexer::new().transform(raw);
let processed = Processor::new().transform(indexed);
```

### With Capabilities

```rust
use tola_vdom::capability::*;
use tola_vdom::phase::Indexed;

// Define a transform that provides a capability
struct LinkChecker;

impl<C: Capabilities> CapTransform<Indexed, C> for LinkChecker {
    type Provides = LinksCheckedCap;
    type Output = <C as AddCapability<LinksCheckedCap>>::Output;

    fn cap_transform(self, doc: Doc<Indexed, C>) -> Doc<Indexed, Self::Output> {
        // Check all links...
        doc.add_capability::<LinksCheckedCap>()
    }
}

// Define a transform that requires a capability
struct LinkResolver;

impl<C, I> CapTransform<Indexed, C> for LinkResolver
where
    C: HasCapability<LinksCheckedCap, I>,  // Requires links to be checked first
{
    type Provides = LinksResolvedCap;
    type Output = <C as AddCapability<LinksResolvedCap>>::Output;

    fn cap_transform(self, doc: Doc<Indexed, C>) -> Doc<Indexed, Self::Output> {
        // Resolve links (safe because they're already checked)
        doc.add_capability::<LinksResolvedCap>()
    }
}

// Usage: Pipeline with compile-time dependency checking
let doc: Doc<Indexed, ()> = Doc::new(indexed_doc);
let doc = LinkChecker.cap_transform(doc);     // Now has LinksCheckedCap
let doc = LinkResolver.cap_transform(doc);    // OK: LinksCheckedCap is present

// This would NOT compile:
// let doc: Doc<Indexed, ()> = Doc::new(indexed_doc);
// let doc = LinkResolver.cap_transform(doc);  // ERROR: missing LinksCheckedCap
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

### User-defined Capabilities

```rust
use tola_vdom::capability::UserCapability;

// Define your own capability
struct MyCustomCap;

impl UserCapability for MyCustomCap {
    const NAME: &'static str = "MyCustom";
}

// Now usable in capability bounds
#[requires(C: MyCustomCap)]
fn needs_custom<C>(doc: Doc<Indexed, C>) { ... }
```

## Requirements

- Rust 1.85+ (edition 2024)
- Typst 0.14+ (for convert module, optional)

## License

MIT
