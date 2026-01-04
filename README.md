# typst-batch

A Typst → HTML batch compilation library with shared global resources.

## ⚠️ Scope Note

This library was extracted from [tola-ssg](https://github.com/tola-ssg/tola-ssg), a Typst-based static site generator. It is specifically designed for **Typst → HTML** workflows and may not be generic enough for all use cases — but feel free to give it a try!

If you need:
- **PDF output** → Use [typst](https://crates.io/crates/typst) directly
- **Single file compilation** → The official `typst-cli` is simpler

## Features

- **Shared fonts**: Loaded once (~100ms saved per compilation)
- **Cached packages**: Downloaded once from Typst registry
- **Incremental builds**: Fingerprint-based file cache invalidation
- **Structured diagnostics**: Rich error messages with source locations
- **Virtual file system**: Inject dynamic content without physical files
- **Metadata extraction**: Query labeled values from compiled documents

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

// Compile a Typst file to HTML
let result = compile_html(Path::new("doc.typ"), Path::new("."))?;
std::fs::write("output.html", &result.html)?;

// Handle diagnostics
for diag in &result.diagnostics {
    eprintln!("{}: {}", diag.severity, diag.message);
}
```

## API Overview

### Compilation

| Function | Description |
|----------|-------------|
| `compile_html` | Compile to HTML bytes |
| `compile_html_with_metadata` | Compile with metadata extraction |
| `compile_document` | Get `HtmlDocument` for further processing |

### Metadata Extraction

```typst
// In your .typ file
#metadata((title: "My Post", date: "2024-01-01")) <post-meta>
```

```rust
let result = compile_html_with_metadata(path, root, "post-meta")?;
println!("Title: {}", result.metadata.unwrap()["title"]);

// Or query multiple labels at once
let map = query_metadata_map(&document, &["meta", "config"]);
```

### Virtual File System

Inject dynamic content accessible via `#json()`, `#read()`, etc:

```rust
use typst_batch::{MapVirtualFS, set_virtual_fs};

let mut vfs = MapVirtualFS::new();
vfs.insert("/_data/site.json", r#"{"title":"My Blog"}"#);
vfs.insert("/_data/posts.json", serde_json::to_string(&posts)?);
set_virtual_fs(vfs);
```

### Diagnostics

```rust
use typst_batch::{DiagnosticOptions, DisplayStyle, format_diagnostics_with_options};

let options = DiagnosticOptions {
    color: true,
    style: DisplayStyle::Rich,  // or DisplayStyle::Short
    ..Default::default()
};

let formatted = format_diagnostics_with_options(&world, &result.diagnostics, &options);
eprintln!("{}", formatted);
```

### Font Configuration

```rust
use typst_batch::{FontOptions, init_fonts_with_options};

let options = FontOptions::new()
    .with_system_fonts(true)
    .with_custom_paths(&[Path::new("assets/fonts")]);

init_fonts_with_options(&options);
```

## Re-exported Types

For convenience, commonly used typst types are re-exported:

- `FileId`, `VirtualPath`, `Source` — File identification
- `SourceDiagnostic`, `DiagnosticSeverity` — Error handling
- `FontBook`, `FontInfo`, `Font` — Font queries
- `typst`, `typst_html`, `typst_kit` — Full crate access

## Requirements

- Typst 0.14.1

## License

MIT
