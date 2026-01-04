//! Diagnostic formatting for Typst compilation errors and warnings.
//!
//! This module provides human-readable formatting for [`SourceDiagnostic`] similar
//! to the official `typst-cli` output.
//!
//! # Two-Layer API
//!
//! This module provides two levels of API via [`DiagnosticsExt`] trait:
//!
//! ## Simple: Ready-to-Use Formatting
//!
//! ```ignore
//! use typst_batch::DiagnosticsExt;
//!
//! let output = result.diagnostics.format(&world);
//! eprintln!("{}", output);
//! ```
//!
//! ## Advanced: Full Customization
//!
//! Use `.resolve()` to get structured [`DiagnosticInfo`] and render
//! it however you want:
//!
//! ```ignore
//! use typst_batch::DiagnosticsExt;
//!
//! for info in result.diagnostics.resolve(&world) {
//!     // Custom rendering: JSON, HTML, IDE integration, etc.
//!     println!("{}: {} at {}:{}",
//!         info.severity, info.message, info.path, info.line);
//! }
//! ```
//!
//! # Example Output
//!
//! ```text
//! error: `invalid:meta.typ` is not a valid package namespace
//!   ┌─ content/index.typ:1:8
//!   │
//! 1 │ #import "@invalid:meta.typ" as meta
//!   │         ^^^^^^^^^^^^^^^^
//! ```

use std::fmt::{self, Write};

use thiserror::Error;
use typst::diag::Severity;
use typst::syntax::Span;
use typst::World;

// Re-export for user convenience
pub use typst::diag::{Severity as DiagnosticSeverity, SourceDiagnostic};

// ============================================================================
// Diagnostic Options
// ============================================================================

/// Display style for diagnostic output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DisplayStyle {
    /// Rich output with source snippets and highlighting.
    #[default]
    Rich,
    /// Short output with just file:line:col and message.
    Short,
}

/// Options for controlling diagnostic formatting.
///
/// # Example
///
/// ```ignore
/// use typst_batch::diagnostic::{DiagnosticOptions, DisplayStyle};
///
/// // Default: colored rich output
/// let opts = DiagnosticOptions::default();
///
/// // Plain text (no ANSI colors) for logging
/// let opts = DiagnosticOptions::plain();
///
/// // Short format for CI/IDE integration
/// let opts = DiagnosticOptions::short();
///
/// // Custom configuration
/// let opts = DiagnosticOptions::new()
///     .with_color(true)
///     .with_style(DisplayStyle::Rich)
///     .with_snippets(true);
/// ```
#[derive(Debug, Clone)]
pub struct DiagnosticOptions {
    /// Whether to use ANSI colors in output.
    pub colored: bool,
    /// Display style (rich with snippets or short).
    pub style: DisplayStyle,
    /// Whether to include source code snippets.
    pub snippets: bool,
    /// Whether to include hints.
    pub hints: bool,
    /// Whether to include trace information.
    pub traces: bool,
    /// Tab width for display (default: 2).
    pub tab_width: usize,
}

impl Default for DiagnosticOptions {
    fn default() -> Self {
        Self {
            colored: true,
            style: DisplayStyle::Rich,
            snippets: true,
            hints: true,
            traces: true,
            tab_width: 2,
        }
    }
}

impl DiagnosticOptions {
    /// Create new options with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create options for colored terminal output.
    pub fn colored() -> Self {
        Self::default()
    }

    /// Create options for plain text output (no ANSI colors).
    pub fn plain() -> Self {
        Self {
            colored: false,
            ..Self::default()
        }
    }

    /// Create options for short format (file:line:col: message).
    pub fn short() -> Self {
        Self {
            style: DisplayStyle::Short,
            snippets: false,
            traces: false,
            ..Self::default()
        }
    }

    /// Set whether to use colors.
    pub fn with_colored(mut self, colored: bool) -> Self {
        self.colored = colored;
        self
    }

    /// Set display style.
    pub fn with_style(mut self, style: DisplayStyle) -> Self {
        self.style = style;
        self
    }

    /// Set whether to include source snippets.
    pub fn with_snippets(mut self, snippets: bool) -> Self {
        self.snippets = snippets;
        self
    }

    /// Set whether to include hints.
    pub fn with_hints(mut self, hints: bool) -> Self {
        self.hints = hints;
        self
    }

    /// Set whether to include traces.
    pub fn with_traces(mut self, traces: bool) -> Self {
        self.traces = traces;
        self
    }

    /// Set tab width for display.
    pub fn with_tab_width(mut self, width: usize) -> Self {
        self.tab_width = width;
        self
    }
}

// ============================================================================
// Compilation Error Type
// ============================================================================

/// Error type for Typst compilation failures.
///
/// This provides structured access to compilation errors for programmatic handling,
/// while also implementing `Display` for human-readable output.
///
/// # Example
///
/// ```ignore
/// match compile_html(path, root) {
///     Ok(result) => { /* success */ }
///     Err(CompileError::Compilation { diagnostics, .. }) => {
///         // Access individual errors
///         for diag in &diagnostics {
///             if diag.severity == Severity::Error {
///                 // Handle error...
///             }
///         }
///     }
///     Err(CompileError::HtmlExport { message }) => {
///         eprintln!("HTML export failed: {message}");
///     }
///     Err(e) => eprintln!("{e}"),
/// }
/// ```
#[derive(Debug, Error)]
pub enum CompileError {
    /// Typst compilation failed with diagnostics.
    #[error("Typst compilation failed:\n{formatted}")]
    Compilation {
        /// The raw diagnostics for programmatic access.
        diagnostics: Vec<SourceDiagnostic>,
        /// Pre-formatted error message.
        formatted: String,
    },

    /// HTML export failed.
    #[error("HTML export failed: {message}")]
    HtmlExport {
        /// Error message from typst_html.
        message: String,
    },

    /// File I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl CompileError {
    /// Create a compilation error from diagnostics.
    pub fn compilation<W: World>(world: &W, diagnostics: Vec<SourceDiagnostic>) -> Self {
        let formatted = format_diagnostics(world, &diagnostics);
        Self::Compilation {
            diagnostics,
            formatted,
        }
    }

    /// Create a compilation error with custom formatting options.
    pub fn compilation_with_options<W: World>(
        world: &W,
        diagnostics: Vec<SourceDiagnostic>,
        options: &DiagnosticOptions,
    ) -> Self {
        let formatted = format_diagnostics_with_options(world, &diagnostics, options);
        Self::Compilation {
            diagnostics,
            formatted,
        }
    }

    /// Create an HTML export error.
    pub fn html_export(message: impl Into<String>) -> Self {
        Self::HtmlExport {
            message: message.into(),
        }
    }

    /// Check if this error contains any fatal errors (vs just warnings).
    pub fn has_fatal_errors(&self) -> bool {
        match self {
            Self::Compilation { diagnostics, .. } => has_errors(diagnostics),
            _ => true,
        }
    }

    /// Get the diagnostics if this is a compilation error.
    pub fn diagnostics(&self) -> Option<&[SourceDiagnostic]> {
        match self {
            Self::Compilation { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }
}

// ============================================================================
// Diagnostic Summary
// ============================================================================

/// Summary of diagnostic counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiagnosticSummary {
    /// Number of errors.
    pub errors: usize,
    /// Number of warnings.
    pub warnings: usize,
}

impl DiagnosticSummary {
    /// Create summary from diagnostics.
    pub fn from_diagnostics(diagnostics: &[SourceDiagnostic]) -> Self {
        let (errors, warnings) = count_diagnostics(diagnostics);
        Self { errors, warnings }
    }

    /// Total number of diagnostics.
    pub fn total(&self) -> usize {
        self.errors + self.warnings
    }

    /// Whether there are any errors.
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    /// Whether there are any diagnostics at all.
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

impl fmt::Display for DiagnosticSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.errors, self.warnings) {
            (0, 0) => write!(f, "no diagnostics"),
            (e, 0) => write!(f, "{e} error{}", if e == 1 { "" } else { "s" }),
            (0, w) => write!(f, "{w} warning{}", if w == 1 { "" } else { "s" }),
            (e, w) => write!(
                f,
                "{e} error{}, {w} warning{}",
                if e == 1 { "" } else { "s" },
                if w == 1 { "" } else { "s" }
            ),
        }
    }
}

// ============================================================================
// Diagnostics Extension Trait
// ============================================================================

/// Extension trait for working with diagnostic slices.
///
/// This trait provides convenient methods for analyzing collections of
/// [`SourceDiagnostic`]s without importing standalone functions.
///
/// # Example
///
/// ```ignore
/// use typst_batch::{compile_html, DiagnosticsExt};
///
/// let result = compile_html(path, root)?;
///
/// if result.diagnostics.has_errors() {
///     eprintln!("Compilation failed with {} errors", result.diagnostics.error_count());
/// }
///
/// let summary = result.diagnostics.summary();
/// println!("{}", summary);  // "2 errors, 1 warning"
///
/// // Format for display
/// let formatted = result.diagnostics.format(&world);
///
/// // Or get structured data for custom rendering
/// let infos = result.diagnostics.resolve(&world);
/// ```
pub trait DiagnosticsExt {
    /// Check if there are any errors in the diagnostics.
    fn has_errors(&self) -> bool;

    /// Check if there are any warnings in the diagnostics.
    fn has_warnings(&self) -> bool;

    /// Check if the diagnostic list is empty.
    fn is_empty(&self) -> bool;

    /// Get the number of diagnostics.
    fn len(&self) -> usize;

    /// Count errors in the diagnostics.
    fn error_count(&self) -> usize;

    /// Count warnings in the diagnostics.
    fn warning_count(&self) -> usize;

    /// Get counts of errors and warnings.
    fn counts(&self) -> (usize, usize);

    /// Get a summary of the diagnostics.
    fn summary(&self) -> DiagnosticSummary;

    /// Filter out diagnostics matching any of the given filters.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use typst_batch::{DiagnosticsExt, DiagnosticFilter};
    ///
    /// // Filter out HTML export warnings and external package warnings
    /// let filtered = diagnostics.filter_out(&[
    ///     DiagnosticFilter::HtmlExport,
    ///     DiagnosticFilter::ExternalPackages,
    /// ]);
    /// ```
    fn filter_out(&self, filters: &[DiagnosticFilter]) -> Vec<SourceDiagnostic>;

    /// Filter out known HTML export development warnings.
    ///
    /// Shorthand for `filter_out(&[DiagnosticFilter::HtmlExport])`.
    fn filter_html_warnings(&self) -> Vec<SourceDiagnostic> {
        self.filter_out(&[DiagnosticFilter::HtmlExport])
    }

    /// Format diagnostics into a human-readable string.
    ///
    /// Uses default options (colored, rich format with snippets).
    fn format<W: World>(&self, world: &W) -> String;

    /// Format diagnostics with custom options.
    fn format_with<W: World>(&self, world: &W, options: &DiagnosticOptions) -> String;

    /// Resolve diagnostics to structured data for custom rendering.
    ///
    /// Use this when you need full control over output format (JSON, HTML, IDE integration, etc.)
    fn resolve<W: World>(&self, world: &W) -> Vec<DiagnosticInfo>;
}

/// Filters for excluding diagnostics.
///
/// Used with [`DiagnosticsExt::filter_out`] to remove unwanted diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticFilter {
    /// Filter out HTML export development warnings.
    ///
    /// Matches: "html export is under active development"
    HtmlExport,

    /// Filter out warnings from external packages (not user code).
    ///
    /// Keeps diagnostics from paths that don't start with `@` (packages).
    ExternalPackages,

    /// Filter out all warnings (keep only errors).
    AllWarnings,

    /// Filter out diagnostics containing specific text in message.
    MessageContains(String),
}

impl DiagnosticFilter {
    /// Check if a diagnostic should be filtered out.
    fn matches(&self, diag: &SourceDiagnostic) -> bool {
        match self {
            DiagnosticFilter::HtmlExport => {
                diag.severity == Severity::Warning
                    && diag.message.contains("html export is under active development")
            }
            DiagnosticFilter::ExternalPackages => {
                diag.severity == Severity::Warning && is_external_package(diag)
            }
            DiagnosticFilter::AllWarnings => diag.severity == Severity::Warning,
            DiagnosticFilter::MessageContains(text) => diag.message.contains(text.as_str()),
        }
    }
}

/// Check if a diagnostic originates from an external package.
fn is_external_package(diag: &SourceDiagnostic) -> bool {
    // Check span's file id for package path
    if let Some(id) = diag.span.id() {
        let path = id.vpath().as_rootless_path();
        // External packages have paths like "@preview/..." or "@local/..."
        path.to_string_lossy().starts_with('@')
    } else {
        false
    }
}

impl DiagnosticsExt for [SourceDiagnostic] {
    fn has_errors(&self) -> bool {
        self.iter().any(|d| d.severity == Severity::Error)
    }

    fn has_warnings(&self) -> bool {
        self.iter().any(|d| d.severity == Severity::Warning)
    }

    fn is_empty(&self) -> bool {
        <[SourceDiagnostic]>::is_empty(self)
    }

    fn len(&self) -> usize {
        <[SourceDiagnostic]>::len(self)
    }

    fn error_count(&self) -> usize {
        self.iter()
            .filter(|d| d.severity == Severity::Error)
            .count()
    }

    fn warning_count(&self) -> usize {
        self.iter()
            .filter(|d| d.severity == Severity::Warning)
            .count()
    }

    fn counts(&self) -> (usize, usize) {
        self.iter().fold((0, 0), |(errors, warnings), d| match d.severity {
            Severity::Error => (errors + 1, warnings),
            Severity::Warning => (errors, warnings + 1),
        })
    }

    fn summary(&self) -> DiagnosticSummary {
        let (errors, warnings) = self.counts();
        DiagnosticSummary { errors, warnings }
    }

    fn filter_out(&self, filters: &[DiagnosticFilter]) -> Vec<SourceDiagnostic> {
        self.iter()
            .filter(|d| !filters.iter().any(|f| f.matches(d)))
            .cloned()
            .collect()
    }

    fn format<W: World>(&self, world: &W) -> String {
        format_diagnostics(world, self)
    }

    fn format_with<W: World>(&self, world: &W, options: &DiagnosticOptions) -> String {
        format_diagnostics_with_options(world, self, options)
    }

    fn resolve<W: World>(&self, world: &W) -> Vec<DiagnosticInfo> {
        self.iter().map(|d| resolve_diagnostic(world, d)).collect()
    }
}

impl DiagnosticsExt for Vec<SourceDiagnostic> {
    fn has_errors(&self) -> bool {
        self.as_slice().has_errors()
    }

    fn has_warnings(&self) -> bool {
        self.as_slice().has_warnings()
    }

    fn is_empty(&self) -> bool {
        Vec::is_empty(self)
    }

    fn len(&self) -> usize {
        Vec::len(self)
    }

    fn error_count(&self) -> usize {
        self.as_slice().error_count()
    }

    fn warning_count(&self) -> usize {
        self.as_slice().warning_count()
    }

    fn counts(&self) -> (usize, usize) {
        self.as_slice().counts()
    }

    fn summary(&self) -> DiagnosticSummary {
        self.as_slice().summary()
    }

    fn filter_out(&self, filters: &[DiagnosticFilter]) -> Vec<SourceDiagnostic> {
        self.as_slice().filter_out(filters)
    }

    fn format<W: World>(&self, world: &W) -> String {
        self.as_slice().format(world)
    }

    fn format_with<W: World>(&self, world: &W, options: &DiagnosticOptions) -> String {
        self.as_slice().format_with(world, options)
    }

    fn resolve<W: World>(&self, world: &W) -> Vec<DiagnosticInfo> {
        self.as_slice().resolve(world)
    }
}

// ============================================================================
// Gutter Characters
// ============================================================================

/// Box-drawing characters for source code display.
mod gutter {
    pub const HEADER: &str = "┌─";
    pub const BAR: &str = "│";
    pub const SPAN_START: &str = "╭";
    pub const SPAN_END: &str = "╰";
    pub const DASH: &str = "─";
    pub const MARKER: &str = "^";
}

// ============================================================================
// Diagnostic Info - Structured data for custom rendering
// ============================================================================

/// Structured diagnostic information for custom rendering.
///
/// This struct contains all the information needed to render a diagnostic
/// in any format (terminal, HTML, JSON, etc.).
///
/// # Example
///
/// ```ignore
/// use typst_batch::diagnostic::{DiagnosticInfo, resolve_diagnostic};
///
/// let info = resolve_diagnostic(&world, &diag);
///
/// // Now you have full control over formatting
/// println!("Error at {}:{}", info.path.unwrap_or("?"), info.line.unwrap_or(0));
/// for line in &info.source_lines {
///     println!("{}: {}", line.line_num, line.text);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct DiagnosticInfo {
    /// Error severity (error or warning).
    pub severity: Severity,
    /// The error message.
    pub message: String,
    /// File path (if available).
    pub path: Option<String>,
    /// Line number (1-indexed, if available).
    pub line: Option<usize>,
    /// Column number (1-indexed, if available).
    pub column: Option<usize>,
    /// Source code lines with highlighting info.
    pub source_lines: Vec<SourceLine>,
    /// Hint messages.
    pub hints: Vec<String>,
    /// Stack trace entries.
    pub traces: Vec<TraceInfo>,
}

/// A source code line with optional highlighting.
#[derive(Debug, Clone)]
pub struct SourceLine {
    /// Line number (1-indexed).
    pub line_num: usize,
    /// The source text.
    pub text: String,
    /// Highlight range (start_col, end_col), 0-indexed.
    pub highlight: Option<(usize, usize)>,
}

/// Stack trace entry.
#[derive(Debug, Clone)]
pub struct TraceInfo {
    /// Description of the trace point (from Typst's Tracepoint::Display).
    pub message: String,
    /// File path (if available).
    pub path: Option<String>,
    /// Line number (1-indexed, if available).
    pub line: Option<usize>,
    /// Column number (1-indexed, if available).
    pub column: Option<usize>,
    /// Source code lines at this trace point.
    pub source_lines: Vec<SourceLine>,
}

/// Resolve a diagnostic to structured info for custom rendering.
/// (Internal - use `diagnostics.resolve(&world)` instead)
pub(crate) fn resolve_diagnostic<W: World>(world: &W, diag: &SourceDiagnostic) -> DiagnosticInfo {
    let location = SpanLocation::from_span(world, diag.span);

    let (path, line, column, source_lines) = match &location {
        Some(loc) => (
            Some(loc.path.clone()),
            Some(loc.start_line),
            Some(loc.start_col + 1), // 1-indexed for display
            loc.to_source_lines(),
        ),
        None => (None, None, None, vec![]),
    };

    let hints = diag.hints.iter().map(|h| h.to_string()).collect();

    let traces = diag
        .trace
        .iter()
        .filter_map(|t| {
            use typst::diag::Tracepoint;

            // Skip import traces - they just show content importing template
            if matches!(t.v, Tracepoint::Import) {
                return None;
            }

            // Use Tracepoint's Display impl for consistent messages
            let message = t.v.to_string();

            let loc = SpanLocation::from_span(world, t.span);
            Some(TraceInfo {
                message,
                path: loc.as_ref().map(|l| l.path.clone()),
                line: loc.as_ref().map(|l| l.start_line),
                column: loc.as_ref().map(|l| l.start_col + 1),
                source_lines: loc.as_ref().map(|l| l.to_source_lines()).unwrap_or_default(),
            })
        })
        .collect();

    DiagnosticInfo {
        severity: diag.severity,
        message: diag.message.to_string(),
        path,
        line,
        column,
        source_lines,
        hints,
        traces,
    }
}

/// Resolve all diagnostics to structured info.
/// (Internal - use `diagnostics.resolve(&world)` instead)
#[allow(dead_code)]
pub(crate) fn resolve_diagnostics<W: World>(
    world: &W,
    diagnostics: &[SourceDiagnostic],
) -> Vec<DiagnosticInfo> {
    diagnostics
        .iter()
        .map(|d| resolve_diagnostic(world, d))
        .collect()
}

// ============================================================================
// Internal Coloring (private)
// ============================================================================

/// Apply color to text based on severity.
#[cfg(feature = "colored-diagnostics")]
fn colorize(text: &str, severity: Severity) -> String {
    use colored::Colorize;
    match severity {
        Severity::Error => text.red().to_string(),
        Severity::Warning => text.yellow().to_string(),
    }
}

#[cfg(feature = "colored-diagnostics")]
fn colorize_help(text: &str) -> String {
    use colored::Colorize;
    text.cyan().to_string()
}

#[cfg(not(feature = "colored-diagnostics"))]
fn colorize(text: &str, _severity: Severity) -> String {
    text.to_owned()
}

#[cfg(not(feature = "colored-diagnostics"))]
fn colorize_help(text: &str) -> String {
    text.to_owned()
}

/// Get paint function based on options.
fn get_paint_fn(options: &DiagnosticOptions, severity: Severity) -> Box<dyn Fn(&str) -> String> {
    if options.colored {
        Box::new(move |s| colorize(s, severity))
    } else {
        Box::new(|s: &str| s.to_owned())
    }
}

fn get_help_paint_fn(options: &DiagnosticOptions) -> Box<dyn Fn(&str) -> String> {
    if options.colored {
        Box::new(colorize_help)
    } else {
        Box::new(|s: &str| s.to_owned())
    }
}

// ============================================================================
// Span Location
// ============================================================================

/// Resolved source location information for a diagnostic span.
///
/// Contains all information needed to display a source code snippet
/// with proper highlighting.
struct SpanLocation {
    /// File path (relative to project root)
    path: String,
    /// Starting line number (1-indexed)
    start_line: usize,
    /// Starting column (0-indexed)
    start_col: usize,
    /// Source lines covered by the span
    lines: Vec<String>,
    /// Column where highlighting starts in first line (0-indexed)
    highlight_start_col: usize,
    /// Column where highlighting ends in last line (0-indexed, exclusive)
    highlight_end_col: usize,
}

impl SpanLocation {
    /// Resolve a span to its source location.
    fn from_span<W: World>(world: &W, span: Span) -> Option<Self> {
        let id = span.id()?;
        let source = world.source(id).ok()?;
        let range = source.range(span)?;
        let text = source.text();

        // Calculate line boundaries
        let start_line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
        let end_line_end = text[range.end..]
            .find('\n')
            .map_or(text.len(), |i| range.end + i);
        let end_line_start = text[..range.end].rfind('\n').map_or(0, |i| i + 1);

        // Calculate positions
        // Column numbers are 0-indexed to match typst-cli output
        let start_line = text[..range.start].matches('\n').count() + 1;
        let start_col = text[start_line_start..range.start].chars().count();
        let end_col = text[end_line_start..range.end].chars().count();

        // Extract source lines
        let lines = text[start_line_start..end_line_end]
            .lines()
            .map(String::from)
            .collect();

        // Build path
        let path = id.vpath().as_rootless_path().to_string_lossy().into_owned();

        Some(Self {
            path,
            start_line,
            start_col,
            lines,
            highlight_start_col: start_col,
            highlight_end_col: end_col,
        })
    }

    /// Check if this span covers multiple lines.
    #[inline]
    const fn is_multiline(&self) -> bool {
        self.lines.len() > 1
    }

    /// Get the last line number covered by this span.
    #[inline]
    const fn end_line(&self) -> usize {
        self.start_line + self.lines.len() - 1
    }

    /// Calculate the width needed to display line numbers.
    #[inline]
    fn line_num_width(&self) -> usize {
        self.end_line().to_string().len().max(1)
    }

    /// Convert to structured source lines for DiagnosticInfo.
    fn to_source_lines(&self) -> Vec<SourceLine> {
        self.lines
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let line_num = self.start_line + i;
                let highlight = if i == 0 {
                    Some((self.highlight_start_col, self.highlight_end_col))
                } else if self.lines.len() > 1 {
                    // Multi-line: highlight entire line after first
                    Some((0, text.chars().count()))
                } else {
                    None
                };
                SourceLine {
                    line_num,
                    text: text.clone(),
                    highlight,
                }
            })
            .collect()
    }
}

// ============================================================================
// Snippet Writer
// ============================================================================

/// Helper for writing formatted source snippets.
///
/// Encapsulates the logic for formatting source code with proper
/// alignment, gutter characters, and highlighting.
struct SnippetWriter<'a, F>
where
    F: Fn(&str) -> String,
{
    output: &'a mut String,
    paint: F,
    line_num_width: usize,
}

impl<'a, F> SnippetWriter<'a, F>
where
    F: Fn(&str) -> String,
{
    fn new(output: &'a mut String, paint: F, line_num_width: usize) -> Self {
        Self {
            output,
            paint,
            line_num_width,
        }
    }

    /// Write the location header: "  ┌─ path:line:col"
    fn write_header(&mut self, path: &str, line: usize, col: usize) {
        _ = writeln!(
            self.output,
            "{:>width$} {} {}:{}:{}",
            "",
            (self.paint)(gutter::HEADER),
            path,
            line,
            col,
            width = self.line_num_width
        );
    }

    /// Write an empty gutter line: "  │"
    fn write_empty_gutter(&mut self) {
        _ = writeln!(
            self.output,
            "{:>width$} {}",
            "",
            (self.paint)(gutter::BAR),
            width = self.line_num_width
        );
    }

    /// Write a source line with optional box character and highlighting.
    fn write_source_line(
        &mut self,
        line_num: usize,
        line_text: &str,
        box_char: Option<&str>,
        highlight_range: Option<(usize, usize)>,
    ) {
        let line_num_str = format!("{:>width$}", line_num, width = self.line_num_width);

        let formatted_line = match (box_char, highlight_range) {
            (Some(bc), Some((start, end))) => {
                let (before, highlighted, after) = split_line(line_text, start, end);
                format!(
                    "{} {} {} {}{}{}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    (self.paint)(bc),
                    before,
                    (self.paint)(&highlighted),
                    after
                )
            }
            (None, Some((start, end))) => {
                let (before, highlighted, after) = split_line(line_text, start, end);
                format!(
                    "{} {} {}{}{}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    before,
                    (self.paint)(&highlighted),
                    after
                )
            }
            _ => {
                format!(
                    "{} {} {}",
                    (self.paint)(&line_num_str),
                    (self.paint)(gutter::BAR),
                    line_text
                )
            }
        };

        _ = writeln!(self.output, "{formatted_line}");
    }

    /// Write marker line for single-line spans: "  │   ^^^^"
    fn write_single_line_marker(&mut self, start_col: usize, span_len: usize) {
        let spaces = " ".repeat(start_col);
        let markers = gutter::MARKER.repeat(span_len.max(1));
        _ = writeln!(
            self.output,
            "{:>width$} {} {}{}",
            "",
            (self.paint)(gutter::BAR),
            spaces,
            (self.paint)(&markers),
            width = self.line_num_width
        );
    }

    /// Write marker line for multi-line spans: "  │ ╰────^"
    fn write_multiline_end_marker(&mut self, end_col: usize) {
        let dashes = gutter::DASH.repeat(end_col);
        _ = writeln!(
            self.output,
            "{:>width$} {} {}{}{}",
            "",
            (self.paint)(gutter::BAR),
            (self.paint)(gutter::SPAN_END),
            (self.paint)(&dashes),
            (self.paint)(gutter::MARKER),
            width = self.line_num_width
        );
    }
}

/// Split a line into (before, highlighted, after) based on column range.
/// Both `start_col` and `end_col` are 0-indexed.
fn split_line(line: &str, start_col: usize, end_col: usize) -> (String, String, String) {
    let chars: Vec<char> = line.chars().collect();
    let start_idx = start_col.min(chars.len());
    let end_idx = end_col.min(chars.len());

    let before: String = chars[..start_idx].iter().collect();
    let highlighted: String = chars[start_idx..end_idx].iter().collect();
    let after: String = chars[end_idx..].iter().collect();

    (before, highlighted, after)
}

// ============================================================================
// Public API
// ============================================================================

/// Format compilation diagnostics into a human-readable string.
/// (Internal - use `diagnostics.format(&world)` instead)
pub(crate) fn format_diagnostics<W: World>(world: &W, diagnostics: &[SourceDiagnostic]) -> String {
    format_diagnostics_with_options(world, diagnostics, &DiagnosticOptions::default())
}

/// Format compilation diagnostics with custom options.
/// (Internal - use `diagnostics.format_with(&world, &options)` instead)
pub(crate) fn format_diagnostics_with_options<W: World>(
    world: &W,
    diagnostics: &[SourceDiagnostic],
    options: &DiagnosticOptions,
) -> String {
    let mut output = String::new();

    // Partition and sort: errors first, then warnings
    let (errors, warnings): (Vec<_>, Vec<_>) = diagnostics
        .iter()
        .partition(|d| d.severity == Severity::Error);

    let all_diags: Vec<_> = errors.iter().chain(warnings.iter()).collect();
    for (i, diag) in all_diags.iter().enumerate() {
        format_diagnostic_internal(&mut output, world, diag, options);
        // Add blank line between diagnostics (but not after the last one)
        if i < all_diags.len() - 1 {
            output.push('\n');
        }
    }

    output
}

/// Count errors and warnings in a diagnostic list.
pub fn count_diagnostics(diagnostics: &[SourceDiagnostic]) -> (usize, usize) {
    diagnostics.iter().fold((0, 0), |(errors, warnings), d| {
        match d.severity {
            Severity::Error => (errors + 1, warnings),
            Severity::Warning => (errors, warnings + 1),
        }
    })
}

/// Check if there are any errors in the diagnostics.
pub fn has_errors(diagnostics: &[SourceDiagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}

/// Filter out known HTML export development warnings.
///
/// Typst's HTML export is experimental and always produces a warning.
/// This function filters out that warning to reduce noise in error output.
pub fn filter_html_warnings(diagnostics: &[SourceDiagnostic]) -> Vec<SourceDiagnostic> {
    diagnostics
        .iter()
        .filter(|d| {
            // Keep all errors
            if d.severity == Severity::Error {
                return true;
            }
            // Filter out HTML export warning
            !d.message.contains("html export is under active development")
        })
        .cloned()
        .collect()
}

/// Disable colored output globally (for tests).
#[cfg(all(test, feature = "colored-diagnostics"))]
pub fn disable_colors() {
    colored::control::set_override(false);
}

// ============================================================================
// Diagnostic Formatting (Internal)
// ============================================================================

/// Format a single diagnostic with its source snippet.
fn format_diagnostic_internal<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    options: &DiagnosticOptions,
) {
    let label = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let paint = get_paint_fn(options, diag.severity);

    match options.style {
        DisplayStyle::Short => {
            format_diagnostic_short(output, world, diag, label, &paint);
        }
        DisplayStyle::Rich => {
            format_diagnostic_rich(output, world, diag, label, &paint, options);
        }
    }
}

/// Format diagnostic in short style: "file:line:col: severity: message"
fn format_diagnostic_short<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    label: &str,
    paint: &dyn Fn(&str) -> String,
) {
    if let Some(loc) = SpanLocation::from_span(world, diag.span) {
        _ = writeln!(
            output,
            "{}:{}:{}: {}: {}",
            loc.path,
            loc.start_line,
            loc.start_col + 1, // 1-indexed for display
            paint(label),
            diag.message
        );
    } else {
        _ = writeln!(output, "{}: {}", paint(label), diag.message);
    }
}

/// Format diagnostic in rich style with source snippets.
fn format_diagnostic_rich<W: World>(
    output: &mut String,
    world: &W,
    diag: &SourceDiagnostic,
    label: &str,
    paint: &dyn Fn(&str) -> String,
    options: &DiagnosticOptions,
) {
    // Header: "error: message"
    _ = writeln!(output, "{}: {}", paint(label), diag.message);

    // Source snippet (if enabled)
    if options.snippets
        && let Some(location) = SpanLocation::from_span(world, diag.span)
    {
        write_snippet(output, &location, paint);
    }

    // Trace information (call stack) - if enabled
    if options.traces {
        let help_paint = get_help_paint_fn(options);
        for trace in &diag.trace {
            write_trace(output, world, &trace.v, trace.span, &help_paint);
        }
    }

    // Hints - if enabled
    if options.hints {
        let help_paint = get_help_paint_fn(options);
        for hint in &diag.hints {
            _ = writeln!(
                output,
                "  {} hint: {}",
                help_paint("="),
                hint
            );
        }
    }
}

/// Write a source code snippet with highlighting.
fn write_snippet(output: &mut String, location: &SpanLocation, paint: &dyn Fn(&str) -> String) {
    let mut writer = SnippetWriter::new(output, |s| paint(s), location.line_num_width());

    writer.write_header(&location.path, location.start_line, location.start_col);
    writer.write_empty_gutter();

    if location.is_multiline() {
        write_multiline_snippet(&mut writer, location);
    } else {
        write_singleline_snippet(&mut writer, location);
    }
}

/// Write a single-line source snippet.
fn write_singleline_snippet<F>(writer: &mut SnippetWriter<F>, location: &SpanLocation)
where
    F: Fn(&str) -> String,
{
    let line_text = location.lines.first().map_or("", String::as_str);

    let span_len = location
        .highlight_end_col
        .saturating_sub(location.highlight_start_col)
        .max(1);

    writer.write_source_line(
        location.start_line,
        line_text,
        None,
        Some((location.highlight_start_col, location.highlight_end_col)),
    );
    writer.write_single_line_marker(location.highlight_start_col, span_len);
}

/// Write a multi-line source snippet with box drawing.
fn write_multiline_snippet<F>(writer: &mut SnippetWriter<F>, location: &SpanLocation)
where
    F: Fn(&str) -> String,
{
    for (i, line_text) in location.lines.iter().enumerate() {
        let line_num = location.start_line + i;
        let is_first = i == 0;
        let line_len = line_text.chars().count();

        let (box_char, highlight_range) = if is_first {
            (
                gutter::SPAN_START,
                (location.highlight_start_col, line_len + 1),
            )
        } else {
            (gutter::BAR, (1, line_len + 1))
        };

        writer.write_source_line(line_num, line_text, Some(box_char), Some(highlight_range));
    }

    writer.write_multiline_end_marker(location.highlight_end_col);
}

/// Write trace information with help theme.
///
/// Skips Import traces since they just show which content file imported
/// the failing template, which is usually not helpful context.
fn write_trace<W: World>(
    output: &mut String,
    world: &W,
    tracepoint: &typst::diag::Tracepoint,
    span: Span,
    help_paint: &dyn Fn(&str) -> String,
) {
    use typst::diag::Tracepoint;

    // Skip import traces - they just show content importing template
    if matches!(tracepoint, Tracepoint::Import) {
        return;
    }

    let message = match tracepoint {
        Tracepoint::Call(Some(name)) => {
            format!("error occurred in this call of function `{name}`")
        }
        Tracepoint::Call(None) => "error occurred in this function call".into(),
        Tracepoint::Show(name) => format!("error occurred in this show rule for `{name}`"),
        Tracepoint::Import => unreachable!(), // Handled above
    };

    _ = writeln!(
        output,
        "{}: {}",
        help_paint("help"),
        message
    );

    if let Some(location) = SpanLocation::from_span(world, span) {
        write_snippet(output, &location, help_paint);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_diagnostics() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error 1"),
            SourceDiagnostic::error(Span::detached(), "error 2"),
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 2"),
        ];

        let (errors, warnings) = count_diagnostics(&diags);
        assert_eq!(errors, 2);
        assert_eq!(warnings, 2);
    }

    #[test]
    fn test_has_errors() {
        let warnings_only = vec![
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::warning(Span::detached(), "warning 2"),
        ];
        assert!(!has_errors(&warnings_only));

        let with_errors = vec![
            SourceDiagnostic::warning(Span::detached(), "warning 1"),
            SourceDiagnostic::error(Span::detached(), "error 1"),
        ];
        assert!(has_errors(&with_errors));

        let empty: Vec<SourceDiagnostic> = vec![];
        assert!(!has_errors(&empty));
    }

    #[test]
    fn test_split_line_helper() {
        // Test normal case: "hello world", cols 6-11 -> "hello " + "world" + ""
        let (before, highlighted, after) = split_line("hello world", 6, 11);
        assert_eq!(before, "hello ");
        assert_eq!(highlighted, "world");
        assert_eq!(after, "");

        // Test start at beginning: "abc", cols 0-1 -> "" + "a" + "bc"
        let (before, highlighted, after) = split_line("abc", 0, 1);
        assert_eq!(before, "");
        assert_eq!(highlighted, "a");
        assert_eq!(after, "bc");

        // Test full line: "test", cols 0-4 -> "" + "test" + ""
        let (before, highlighted, after) = split_line("test", 0, 4);
        assert_eq!(before, "");
        assert_eq!(highlighted, "test");
        assert_eq!(after, "");

        // Test with Unicode: "你好世界" (Hello World), cols 0-2 -> "" + "你好" + "世界"
        let (before, highlighted, after) = split_line("你好世界", 0, 2);
        assert_eq!(before, "");
        assert_eq!(highlighted, "你好");
        assert_eq!(after, "世界");
    }

    #[test]
    fn test_span_location_methods() {
        let location = SpanLocation {
            path: "test.typ".to_string(),
            start_line: 10,
            start_col: 0, // 0-indexed
            lines: vec!["line1".into(), "line2".into(), "line3".into()],
            highlight_start_col: 0,
            highlight_end_col: 5,
        };

        assert!(location.is_multiline());
        assert_eq!(location.end_line(), 12);
        assert_eq!(location.line_num_width(), 2);

        let single_line = SpanLocation {
            path: "test.typ".to_string(),
            start_line: 5,
            start_col: 0, // 0-indexed
            lines: vec!["single".into()],
            highlight_start_col: 0,
            highlight_end_col: 6,
        };

        assert!(!single_line.is_multiline());
        assert_eq!(single_line.end_line(), 5);
        assert_eq!(single_line.line_num_width(), 1);
    }

    #[test]
    fn test_filter_html_warnings() {
        let diags = vec![
            SourceDiagnostic::error(Span::detached(), "error 1"),
            SourceDiagnostic::warning(
                Span::detached(),
                "html export is under active development",
            ),
            SourceDiagnostic::warning(Span::detached(), "other warning"),
        ];

        let filtered = filter_html_warnings(&diags);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|d| d.message == "error 1"));
        assert!(filtered.iter().any(|d| d.message == "other warning"));
    }
}
