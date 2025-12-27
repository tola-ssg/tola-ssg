//! Global config with atomic reload support.
//!
//! Uses `arc-swap` for lock-free reads and atomic config replacement.
//! This enables hot-reloading of `tola.toml` during watch mode.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    CONFIG (ArcSwap)                         │
//! │                                                             │
//! │  ┌─────────────┐     ┌─────────────┐     ┌─────────────┐    │
//! │  │  Reader 1   │     │  Reader 2   │     │   Writer    │    │
//! │  │  (rayon)    │     │  (rayon)    │     │  (watch)    │    │
//! │  └──────┬──────┘     └──────┬──────┘     └──────┬──────┘    │
//! │         │                   │                   │           │
//! │         ▼                   ▼                   ▼           │
//! │      config()            config()         reload_config()   │
//! │    (lock-free)         (lock-free)      (atomic replace)    │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use super::SiteConfig;
use crate::cli::Cli;
use arc_swap::{ArcSwap, Guard};
use std::sync::{Arc, LazyLock};

// =============================================================================
// Global State
// =============================================================================

/// Global config storage with atomic replacement support.
///
/// Initialized with default config, then replaced with loaded config in main.
/// During watch mode, can be atomically replaced when tola.toml changes.
pub static CONFIG: LazyLock<ArcSwap<SiteConfig>> =
    LazyLock::new(|| ArcSwap::from_pointee(SiteConfig::default()));

// =============================================================================
// Type Aliases
// =============================================================================

/// Reference to the current config.
///
/// This type auto-derefs to `&SiteConfig`, so functions accepting `&SiteConfig`
/// work transparently with this type.
pub type ConfigRef = Guard<Arc<SiteConfig>>;

// =============================================================================
// Public API
// =============================================================================

/// Get current config (lock-free read).
///
/// Returns a guard that keeps the config alive until dropped.
/// Thread-safe and wait-free - suitable for use in rayon parallel contexts.
#[inline]
pub fn config() -> ConfigRef {
    CONFIG.load()
}

/// Replace config atomically (called when tola.toml changes).
///
/// The old config remains valid for any readers that loaded it before this call.
/// New readers will see the updated config.
pub fn reload_config(cli: &'static Cli) -> anyhow::Result<()> {
    let new_config = SiteConfig::load(cli)?;
    CONFIG.store(Arc::new(new_config));
    Ok(())
}

/// Initialize global config (called once at startup).
///
/// This replaces the default config with the loaded one.
pub fn init_config(config: SiteConfig) {
    CONFIG.store(Arc::new(config));
}
