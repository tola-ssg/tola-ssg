# typst-batch

A Typst → HTML batch compilation library with shared global resources.

## ⚠️ Scope Note

This library was created for [tola](https://github.com/tola-ssg/tola-ssg), a Typst-based static site generator. It is specifically designed for **Typst → HTML** workflows and may not suit all use cases.

If you need:
- **PDF output** → Use [typst](https://crates.io/crates/typst) directly or the official `typst-cli`
- **Single file compilation** → The official `typst-cli` is simpler
- **Custom output formats** → Use the typst crate directly

## Overview

`typst-batch` optimizes batch compilation by sharing expensive resources:

- **Fonts**: Loaded once (~100ms saved per compilation)
- **Packages**: Downloaded once from Typst registry and cached
- **File cache**: Fingerprint-based invalidation for incremental builds
- **Standard library**: Shared instance with HTML feature enabled

## Installation

```toml
[dependencies]
typst-batch = "0.1"
```

## Quick Start

```rust
use typst_batch::{compile_html, get_fonts};
use std::path::Path;

// Initialize fonts once at startup
get_fonts(&[]);

// Compile a single file
let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
std::fs::write("output.html", &result.html)?;
```

## High-Level API

### Compile to HTML

```rust
use typst_batch::compile_html;

let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
// result.html: Vec<u8>
// result.accessed_files: Vec<PathBuf>
// result.warnings: Option<String>
```

### Compile with Metadata Extraction

In your Typst file:
```typst
#metadata((title: "My Post", date: "2024-01-01")) <post-meta>
```

Then extract it:
```rust
use typst_batch::compile_html_with_metadata;

let result = compile_html_with_metadata(
    Path::new("post.typ"),
    Path::new("."),
    "post-meta",  // label name (without angle brackets)
)?;

if let Some(meta) = &result.metadata {
    println!("Title: {}", meta["title"]);
}
```

### Get HtmlDocument for Further Processing

```rust
use typst_batch::compile_document;

let result = compile_document(Path::new("doc.typ"), Path::new("."))?;
// result.document: typst_html::HtmlDocument
// Process with tola-vdom or other libraries
```

### Query Metadata from Existing Document

```rust
use typst_batch::query_metadata;

let meta = query_metadata(&document, "post-meta");
```

## Configuration (Optional)

```rust
use typst_batch::config::ConfigBuilder;

// Custom User-Agent for package downloads (default: "typst-batch/{version}")
ConfigBuilder::new()
    .user_agent("my-app/1.0.0")
    .init();
```

## Virtual Files

Support dynamically generated files that don't exist on disk:

```rust
use typst_batch::{set_virtual_provider, VirtualDataProvider};
use std::path::Path;

struct MyVirtualData;

impl VirtualDataProvider for MyVirtualData {
    fn is_virtual_path(&self, path: &Path) -> bool {
        path.starts_with("/_data/")
    }

    fn read_virtual(&self, path: &Path) -> Option<Vec<u8>> {
        if path == Path::new("/_data/config.json") {
            Some(b"{}".to_vec())
        } else {
            None
        }
    }
}

set_virtual_provider(MyVirtualData);
```

## Low-Level API

For advanced use cases, access the underlying typst crates:

```rust
use typst_batch::{typst, typst_html, SystemWorld};

let world = SystemWorld::new(path, root);
let result = typst::compile(&world);
let html_doc = typst_html::html(&result.output.unwrap())?;
```

## Requirements

- Rust 1.85+ (edition 2024)
- Typst 0.14.1

## License

MIT
