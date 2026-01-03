//! Hot Reload Pipeline
//!
//! This module contains the business logic for the hot reload system,
//! separate from the Actor concurrency primitives.
//!
//! # Design Philosophy
//!
//! - `actor/` → Pure concurrency primitives (message loops, channels)
//! - `pipeline/` → Business logic (compile, diff, cache)
//!
//! This separation allows:
//! 1. Reuse: `watch.rs` (blocking) and `actor/` (async) share the same logic
//! 2. Testing: Pipeline functions can be tested without Actor machinery
//! 3. Clarity: Actor code focuses on "when/how to run", pipeline on "what to do"

#![allow(dead_code)] // Allow unused during migration

pub mod compile;
pub mod diff;

// Re-exports for convenience
