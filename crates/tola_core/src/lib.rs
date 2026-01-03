//! tola_core - Core infrastructure for document processing pipelines
//!
//! This crate provides foundational traits and utilities that are shared
//! across the tola ecosystem, without any domain-specific logic.
//!
//! # Design Philosophy
//!
//! - **No Diagnostics**: Each domain (typst, vdom) defines its own diagnostic
//!   types because "location" means different things (source span vs DOM node).
//! - **No Actor Model**: Concurrency is an application-level concern.
//! - **Pure Traits**: All interfaces are designed for pure function implementations.
//!
//! # Modules
//!
//! - [`transform`] - Generic transform trait for pipeline composition
//! - [`cache`] - Cache trait abstraction for memoization
//! - [`hash`] - Content-based hashing utilities

pub mod cache;
pub mod hash;
pub mod transform;

pub use cache::Cache;
pub use transform::Transform;
