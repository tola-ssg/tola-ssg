//! # tola_typst
//!
//! Typst integration for the tola static site generator.
//!
//! This crate provides:
//! - [`World`] implementation for Typst compilation
//! - Font discovery and management
//! - Package resolution
//! - Diagnostic formatting
//!
//! ## Modules
//!
//! - [`world`]: Typst World implementation
//! - [`font`]: Font discovery and loading
//! - [`package`]: Package resolution
//! - [`diagnostic`]: Error formatting
//! - [`file`]: File system abstraction

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod diagnostic;
pub mod file;
pub mod font;
pub mod library;
pub mod package;
pub mod world;

pub use world::TolaWorld;
