//! Hot Reload Pipeline
//!
//! Business logic for the hot reload system, separate from Actor concurrency.
//!
//! # Modules
//!
//! - `classify` - File categorization and dependency resolution
//! - `compile` - Typst to VDOM compilation
//! - `diff` - VDOM diffing for incremental updates
//! - `init` - Initial build for cache population
//!
//! # Design
//!
//! - `actor/` → Concurrency (message loops, channels)
//! - `pipeline/` → Business logic (pure functions)

pub mod classify;
pub mod compile;
pub mod diff;
pub mod init;
