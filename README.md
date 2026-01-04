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
use typst_batch::prelude::*;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize fonts once at startup (searches system fonts)
    get_fonts(&[]);

    let path = Path::new("doc.typ");
    let root = Path::new(".");

    // Compile a Typst file to HTML
    let result = compile_html(path, root)?;

    // Check for errors using trait method
    if result.diagnostics.has_errors() {
        let world = SystemWorld::new(path, root);
        eprintln!("Compilation failed:");
        eprintln!("{}", format_diagnostics(&world, &result.diagnostics));
        return Err("compilation error".into());
    }

    // Write output
    std::fs::write("output.html", &result.html)?;
    println!("Compiled successfully! ({} bytes)", result.html.len());

    // Print summary if there are warnings
    let summary = result.diagnostics.summary();
    if !summary.is_empty() {
        eprintln!("{}", summary);  // e.g., "2 warnings"
    }

    Ok(())
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

Extract structured data from your Typst documents using labels:

**In your `.typ` file:**
```typst
#metadata((
  title: "My Blog Post",
  date: "2024-01-01",
  tags: ("rust", "typst"),
)) <post-meta>

= My Blog Post

This is the content...
```

**In Rust:**
```rust
use typst_batch::compile_html_with_metadata;
use std::path::Path;

let result = compile_html_with_metadata(
    Path::new("post.typ"),
    Path::new("."),
    "post-meta",  // label name without angle brackets
)?;

// Access metadata as serde_json::Value
if let Some(meta) = &result.metadata {
    println!("Title: {}", meta["title"]);
    println!("Date: {}", meta["date"]);

    // Access arrays
    if let Some(tags) = meta["tags"].as_array() {
        println!("Tags: {:?}", tags);
    }
}
```

**Query multiple labels at once:**
```rust
use typst_batch::{compile_document, query_metadata_map};

let doc_result = compile_document(path, root)?;
let metadata_map = query_metadata_map(&doc_result.document, &["post-meta", "site-config"]);

if let Some(post) = metadata_map.get("post-meta") {
    println!("Post title: {}", post["title"]);
}
if let Some(config) = metadata_map.get("site-config") {
    println!("Site name: {}", config["name"]);
}
```

### Virtual File System

Inject dynamic content that doesn't exist on disk. Virtual files are accessible
in Typst via `#json()`, `#read()`, `#yaml()`, etc.

**Simple usage with `MapVirtualFS`:**
```rust
use typst_batch::{MapVirtualFS, set_virtual_fs};

let mut vfs = MapVirtualFS::new();

// Inject JSON data
vfs.insert("/_data/site.json", r#"{"title":"My Blog","url":"https://example.com"}"#);

// Inject computed data
let posts_json = serde_json::to_string(&posts)?;
vfs.insert("/_data/posts.json", &posts_json);

// Register globally (call once at startup)
set_virtual_fs(vfs);
```

**In your `.typ` file:**
```typst
#let site = json("/_data/site.json")
#let posts = json("/_data/posts.json")

= #site.title

#for post in posts [
  - #link(post.url)[#post.title]
]
```

**Custom VFS implementation:**
```rust
use typst_batch::{VirtualFileSystem, set_virtual_fs};
use std::path::Path;

struct DynamicVFS {
    build_time: String,
}

impl VirtualFileSystem for DynamicVFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        match path.to_str()? {
            "/_data/build.json" => {
                let json = format!(r#"{{"time":"{}"}}"#, self.build_time);
                Some(json.into_bytes())
            }
            _ => None, // Fall back to real filesystem
        }
    }
}

set_virtual_fs(DynamicVFS {
    build_time: chrono::Utc::now().to_rfc3339()
});
```

### Diagnostics

Format compilation errors and warnings with source context:

```rust
use typst_batch::{
    compile_html, DiagnosticOptions, DisplayStyle,
    format_diagnostics_with_options, DiagnosticsExt, SystemWorld,
};
use std::path::Path;

let path = Path::new("doc.typ");
let root = Path::new(".");
let world = SystemWorld::new(path, root);
let result = compile_html(path, root)?;

// Use trait methods on diagnostics
if result.diagnostics.has_errors() {
    eprintln!("Found {} errors", result.diagnostics.error_count());
}

// Get summary
let summary = result.diagnostics.summary();
println!("{}", summary);  // "2 errors, 1 warning"

// Or get raw counts
let (errors, warnings) = result.diagnostics.counts();

// Format with options
let options = DiagnosticOptions {
    color: true,                    // ANSI colors
    style: DisplayStyle::Rich,      // Full source snippets
    hints: true,                    // Include hints
    ..Default::default()
};

let formatted = format_diagnostics_with_options(&world, &result.diagnostics, &options);
eprintln!("{}", formatted);

// Short style for CI/IDE (file:line:col: message)
let short_options = DiagnosticOptions::short();

// Filter out noisy HTML export warnings
let filtered = result.diagnostics.filter_html_warnings();
```

### Font Configuration

```rust
use typst_batch::{FontOptions, init_fonts_with_options, get_fonts, font_count};
use std::path::Path;

// Option 1: Simple initialization with system fonts
get_fonts(&[]);

// Option 2: With custom font directories
get_fonts(&[Path::new("assets/fonts"), Path::new("content/fonts")]);

// Option 3: Detailed configuration
let options = FontOptions::new()
    .with_system_fonts(true)           // Include system fonts
    .with_custom_paths(&[              // Add custom directories
        Path::new("assets/fonts"),
    ]);

init_fonts_with_options(&options);

// Check loaded fonts
if let Some(count) = font_count() {
    println!("Loaded {} fonts", count);
}
```

## Re-exported Types

For convenience, commonly used typst types are re-exported:

- `FileId`, `VirtualPath`, `Source` — File identification
- `SourceDiagnostic`, `DiagnosticSeverity`, `DiagnosticsExt` — Error handling
- `FontBook`, `FontInfo`, `Font` — Font queries
- `typst`, `typst_html`, `typst_kit` — Full crate access

## Requirements

- Typst 0.14.1

## License

MIT
