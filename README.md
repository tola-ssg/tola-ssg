# typst-batch

A Typst compilation library with shared global resources for batch processing.

## Overview

`typst-batch` provides a [`World`](https://docs.rs/typst/latest/typst/trait.World.html) implementation optimized for compiling multiple Typst documents efficiently. Resources are loaded once and shared across all compilations:

- **Fonts**: System and custom fonts loaded once at startup
- **Packages**: Downloaded once from the Typst registry and cached
- **File cache**: Fingerprint-based invalidation for incremental builds
- **Standard library**: Shared instance with HTML feature enabled

## Installation

```toml
[dependencies]
typst-batch = "0.1"
```

## Usage

### Basic Usage

```rust
use typst_batch::{SystemWorld, get_fonts};
use std::path::Path;

// Initialize fonts (once at startup)
let _fonts = get_fonts(&[]);

// Create a world for compilation
let world = SystemWorld::new(
    Path::new("document.typ"),
    Path::new("."),
);

// Compile with typst
let result = typst::compile(&world);
```

### Configuration (Optional)

Configuration is optional. Without any configuration, the library uses sensible defaults:

```rust
// Option 1: Use defaults (recommended for most cases)
// No configuration needed - just start using the library

// Option 2: Custom User-Agent for package downloads
use typst_batch::config::ConfigBuilder;

ConfigBuilder::new()
    .user_agent("my-app/1.0.0")
    .init();
```

The only configurable option is `user_agent`, which is used for HTTP requests when downloading packages from the Typst registry. Default: `"typst-batch/{version}"`.

### Virtual Files

Support dynamically generated files that don't exist on disk:

```rust
use typst_batch::file::{set_virtual_provider, VirtualDataProvider};
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

### Incremental Compilation

The file cache automatically tracks file access and invalidates based on content fingerprints:

```rust
use typst_batch::file::{reset_access_flags, get_accessed_files, clear_file_cache};

// Before each compilation
reset_access_flags();

// ... compile ...

// Get files accessed during compilation
let accessed = get_accessed_files();

// Clear cache when dependencies change
clear_file_cache();
```

## Modules

| Module | Description |
|--------|-------------|
| `config` | Runtime configuration (User-Agent, project defaults) |
| `world` | `SystemWorld` - Typst World implementation |
| `font` | Global font discovery and loading |
| `package` | Package resolution and caching |
| `library` | Typst standard library with HTML feature |
| `file` | File caching, virtual files, access tracking |
| `diagnostic` | Error and warning formatting |

## Requirements

- Rust 1.85+ (edition 2024)
- Typst 0.14.1

## License

MIT
