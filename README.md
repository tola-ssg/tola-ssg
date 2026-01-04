# typst-batch

A Typst → HTML batch compilation library with shared global resources.

This library was extracted from [tola-ssg](https://github.com/tola-ssg/tola-ssg), a Typst-based static site generator. It is specifically designed for **Typst → HTML** workflows and may not be generic enough for all use cases — but feel free to give it a try!

Of course, it also works well for **single document compilation** when you need features like virtual file injection or friendly helper functions.

If you need:
- **PDF output** → Use [typst](https://crates.io/crates/typst) directly
- **Single file compilation** → The official `typst-cli` is simpler (unless you need VFS features)

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

# Optional: Enable colored diagnostics (requires `colored` crate)
# typst-batch = { version = "0.1", features = ["colored-diagnostics"] }
```

### Features

| Feature | Description |
|---------|-------------|
| `colored-diagnostics` | Enable ANSI colored output via `colored` crate |

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
        eprintln!("{}", result.diagnostics.format(&world));
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

### Document Inputs (sys.inputs)

Pass runtime data to Typst documents via `sys.inputs`:

**In Rust:**
```rust
use typst_batch::compile_html_with_inputs;
use std::path::Path;

// Simple key-value pairs
let result = compile_html_with_inputs(
    Path::new("doc.typ"),
    Path::new("."),
    [("title", "Hello World"), ("author", "Alice")],
)?;
```

**In your `.typ` file:**
```typst
#let title = sys.inputs.at("title", default: "Untitled")
#let author = sys.inputs.at("author", default: "Unknown")

= #title
by _#author_
```

#### Performance Considerations

Using `sys.inputs` creates a **new library instance** per compilation, which
has a small performance overhead. For batch compilation of many documents:

**Recommended for static/global data:** Use VFS (Virtual File System) to inject
shared data as files. This allows all compilations to share the global library:

```rust
use typst_batch::{MapVirtualFS, set_virtual_fs, compile_html};

let mut vfs = MapVirtualFS::new();
vfs.insert("/_data/site.json", r#"{"name":"My Blog"}"#);
set_virtual_fs(vfs);

// All compilations share the global library - best performance
let result = compile_html(path, root)?;
```

**Use `sys.inputs` for:** Build-time variables, CLI arguments, or truly
document-specific data that can't be pre-computed as VFS files.

**Low-level API with `SystemWorld`:**
```rust
use typst_batch::{SystemWorld, create_library_with_inputs};
use typst_batch::typst::foundations::{Dict, IntoValue};

// Builder pattern
let world = SystemWorld::new(path, root)
    .with_inputs([("key", "value")]);

// Or with pre-built Dict
let mut inputs = Dict::new();
inputs.insert("key".into(), "value".into_value());
let world = SystemWorld::new(path, root)
    .with_inputs_dict(inputs);

// Then compile with typst::compile(&world)
```

**Performance Note:** Using `sys.inputs` creates a new library instance per
compilation, bypassing the shared global library. For batch compilation without
inputs, use the standard `compile_html()` which shares resources.

### Virtual File System

The VFS allows you to inject dynamic content that doesn't exist on disk, enabling
flexibility and extensibility that would be difficult to achieve in Typst alone.

**Use cases:**
- Inject computed data (post lists, site config, build timestamps)
- Provide per-document context without modifying source files
- Implement template inheritance patterns
- Share data between documents without file I/O

**Compatibility Note:** VFS is a non-standard extension. Documents using virtual
files won't compile with standard `typst-cli`. Consider this trade-off for your use case.

Virtual files are accessible in Typst via `#json()`, `#read()`, `#yaml()`, etc.

**Simple usage with `MapVirtualFS`:**
```rust
use typst_batch::{MapVirtualFS, set_virtual_fs};

let mut vfs = MapVirtualFS::new();

// Inject JSON data
vfs.insert("/_data/site.json", r#"{"title":"My Blog", "url":"https://example.com"}"#);

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

struct BuildInfoVFS {
    build_time: String,
    version: String,
}

impl VirtualFileSystem for BuildInfoVFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        match path.to_str()? {
            "/_meta/build.json" => {
                let json = serde_json::json!({
                    "time": self.build_time,
                    "version": self.version,
                });
                Some(json.to_string().into_bytes())
            }
            _ => None, // Fall back to real filesystem
        }
    }
}

set_virtual_fs(BuildInfoVFS {
    build_time: chrono::Utc::now().to_rfc3339(),
    version: env!("CARGO_PKG_VERSION").into(),
});
```

**Chained VFS - combine multiple providers:**
```rust
use typst_batch::{VirtualFileSystem, set_virtual_fs};
use std::path::Path;

/// A VFS that chains multiple providers, trying each in order.
struct ChainedVFS {
    providers: Vec<Box<dyn VirtualFileSystem>>,
}

impl ChainedVFS {
    fn new() -> Self {
        Self { providers: Vec::new() }
    }

    fn add<V: VirtualFileSystem + 'static>(mut self, vfs: V) -> Self {
        self.providers.push(Box::new(vfs));
        self
    }
}

impl VirtualFileSystem for ChainedVFS {
    fn read(&self, path: &Path) -> Option<Vec<u8>> {
        // Try each provider in order, return first match
        self.providers.iter().find_map(|p| p.read(path))
    }
}

// Usage: combine site config + per-document data
let vfs = ChainedVFS::new()
    .add(site_config_vfs)
    .add(post_metadata_vfs)
    .add(build_info_vfs);

set_virtual_fs(vfs);
```

### Diagnostics

Format compilation errors and warnings with source context:

```rust
use typst_batch::{
    compile_html, DiagnosticOptions, DisplayStyle,
    DiagnosticsExt, SystemWorld,
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

// Format diagnostics (default: colored with rich snippets)
let formatted = result.diagnostics.format(&world);
eprintln!("{}", formatted);

// Format with custom options
let options = DiagnosticOptions {
    colored: true,                  // ANSI colors (requires `colored-diagnostics` feature)
    style: DisplayStyle::Rich,      // Full source snippets
    hints: true,                    // Include hints
    ..Default::default()
};
let formatted = result.diagnostics.format_with(&world, &options);

// Short style for CI/IDE (file:line:col: message)
let short = result.diagnostics.format_with(&world, &DiagnosticOptions::short());

// Filter out unwanted diagnostics
use typst_batch::DiagnosticFilter;

// Filter out HTML export warnings (shorthand)
let filtered = result.diagnostics.filter_html_warnings();

// Or use the general filter API for more control
let filtered = result.diagnostics.filter_out(&[
    DiagnosticFilter::HtmlExport,       // "html export is under active development"
    DiagnosticFilter::ExternalPackages, // Warnings from extermal packages like `@preview/...`, `@local/...`
]);

// Available filters:
// - DiagnosticFilter::HtmlExport         - HTML export development warning
// - DiagnosticFilter::ExternalPackages   - Warnings from external packages (@preview/...)
// - DiagnosticFilter::AllWarnings        - All warnings (keep only errors)
// - DiagnosticFilter::MessageContains(s) - Custom filter by message text
```

**Full Customization with Structured Data:**

For complete control over rendering (JSON, HTML, IDE integration, etc.),
use `.resolve()` to get structured `DiagnosticInfo`:

```rust
use typst_batch::{DiagnosticsExt, DiagnosticInfo};

// Resolve all diagnostics to structured data
for info in result.diagnostics.resolve(&world) {
    // Full access to all diagnostic data:
    // - info.severity: Error or Warning
    // - info.message: Error message
    // - info.path: Source file path (if available)
    // - info.line, info.column: Location (1-indexed, if available)
    // - info.source_lines: Vec<SourceLine> with line_num, text, highlight range
    // - info.hints: List of hint strings
    // - info.traces: Vec<TraceInfo>, each with:
    //     - message: Trace point description
    //     - path, line, column: Location (if available)
    //     - source_lines: Source context at this trace point

    // Example: Custom JSON output
    println!("{}", serde_json::to_string_pretty(&info)?);

    // Example: Custom HTML output
    println!(r#"<div class="{:?}">"#, info.severity);
    println!("  <strong>{}:{}</strong> {}",
        info.path.as_deref().unwrap_or(""),
        info.line.unwrap_or(0),
        info.message);
    for line in &info.source_lines {
        println!("  <code>{}</code>", line.text);
    }
    println!("</div>");
}
```

The `DiagnosticInfo` struct contains:
- `severity`: `Severity::Error` or `Severity::Warning`
- `message`: The diagnostic message
- `path`: Source file path (optional)
- `line`, `column`: Location info (optional)
- `source_lines`: Vec of `SourceLine` with line number, text, and highlight range
- `hints`: Vec of hint strings
- `traces`: Vec of `TraceInfo` with call stack details

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

## Typst Access

For advanced use cases, access the full typst ecosystem:

```rust
// Access any typst type via the re-exported crate
use typst_batch::typst::syntax::{FileId, VirtualPath, Source};
use typst_batch::typst::diag::{SourceDiagnostic, Severity};
use typst_batch::typst::foundations::{Dict, IntoValue};
use typst_batch::typst::text::{FontBook, FontInfo, Font};

// Or use the full crates
use typst_batch::typst_html;
use typst_batch::typst_kit;
```

## Requirements

- Typst 0.14.1

## License

MIT
