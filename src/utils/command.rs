//! External command execution utilities.
//!
//! Provides macros and functions for running shell commands with proper
//! output handling and error reporting.

use crate::log;
use anyhow::{Context, Result};
use std::{
    ffi::OsString,
    path::Path,
    process::{Child, ChildStdin, Command, Output, Stdio},
};

// ============================================================================
// Macros
// ============================================================================

/// Run an external command with arguments.
///
/// # Examples
/// ```ignore
/// // Without working directory
/// exec!(["git"]; "status", "-s")?;
///
/// // With working directory
/// exec!(root; ["typst"]; "compile", input, output)?;
/// ```
#[macro_export]
macro_rules! exec {
    ($cmd:expr; $($arg:expr),* $(,)?) => {{
        $crate::utils::command::exec(
            None,
            &$crate::utils::command::to_cmd_vec($cmd),
            &$crate::utils::command::filter_args(&[$($crate::utils::command::to_os($arg)),*]),
        )
    }};
    ($root:expr; $cmd:expr; $($arg:expr),* $(,)?) => {{
        $crate::utils::command::exec(
            Some($root),
            &$crate::utils::command::to_cmd_vec($cmd),
            &$crate::utils::command::filter_args(&[$($crate::utils::command::to_os($arg)),*]),
        )
    }};
}

/// Run an external command and return a handle to its stdin.
///
/// The child process is spawned with stdin piped, stdout/stderr nulled.
/// Caller is responsible for writing to stdin and dropping it to signal EOF.
#[macro_export]
macro_rules! exec_with_stdin {
    ($cmd:expr; $($arg:expr),* $(,)?) => {{
        $crate::utils::command::spawn_with_stdin(
            None,
            &$crate::utils::command::to_cmd_vec($cmd),
            &$crate::utils::command::filter_args(&[$($crate::utils::command::to_os($arg)),*]),
        )
    }};
    ($root:expr; $cmd:expr; $($arg:expr),* $(,)?) => {{
        $crate::utils::command::spawn_with_stdin(
            Some($root),
            &$crate::utils::command::to_cmd_vec($cmd),
            &$crate::utils::command::filter_args(&[$($crate::utils::command::to_os($arg)),*]),
        )
    }};
}

// ============================================================================
// Argument Conversion
// ============================================================================

/// Convert to OsString.
#[inline]
pub fn to_os<S: Into<OsString>>(s: S) -> OsString {
    s.into()
}

/// Trait for converting to command vector.
pub trait ToCmd {
    fn to_cmd(self) -> Vec<OsString>;
}

impl<const N: usize> ToCmd for [&str; N] {
    #[inline]
    fn to_cmd(self) -> Vec<OsString> {
        self.into_iter().map(OsString::from).collect()
    }
}

impl ToCmd for &[String] {
    #[inline]
    fn to_cmd(self) -> Vec<OsString> {
        self.iter().map(OsString::from).collect()
    }
}

impl ToCmd for &Vec<String> {
    #[inline]
    fn to_cmd(self) -> Vec<OsString> {
        self.iter().map(OsString::from).collect()
    }
}

/// Convert command to Vec<OsString>.
#[inline]
pub fn to_cmd_vec<C: ToCmd>(cmd: C) -> Vec<OsString> {
    cmd.to_cmd()
}

/// Filter out empty args.
#[inline]
pub fn filter_args(args: &[OsString]) -> Vec<OsString> {
    args.iter().filter(|a| !a.is_empty()).cloned().collect()
}

// ============================================================================
// Command Execution
// ============================================================================

/// Execute a command and capture its output.
///
/// # Errors
/// Returns error if command fails to execute or returns non-zero exit code.
pub fn exec(root: Option<&Path>, cmd: &[OsString], args: &[OsString]) -> Result<Output> {
    let (name, mut command) = prepare(root, cmd, args)?;

    let output = command
        .output()
        .with_context(|| format!("Failed to execute `{name}`"))?;

    log_output(&name, &output)?;
    Ok(output)
}

/// Spawn a command and return a handle to write to its stdin.
///
/// # Errors
/// Returns error if command fails to spawn.
pub fn spawn_with_stdin(
    root: Option<&Path>,
    cmd: &[OsString],
    args: &[OsString],
) -> Result<ChildStdin> {
    let (name, mut command) = prepare(root, cmd, args)?;

    command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let mut child: Child = command
        .spawn()
        .with_context(|| format!("Failed to spawn `{name}`"))?;

    child.stdin.take().context("Failed to acquire stdin")
}

/// Prepare a Command from components.
fn prepare(root: Option<&Path>, cmd: &[OsString], args: &[OsString]) -> Result<(String, Command)> {
    let name = cmd
        .first()
        .and_then(|s| s.to_str())
        .context("Empty command")?
        .to_owned();

    let mut command = Command::new(&cmd[0]);
    command.args(&cmd[1..]).args(args);

    if let Some(dir) = root {
        command.current_dir(dir);
    }

    Ok((name, command))
}

// ============================================================================
// Output Filtering
// ============================================================================

/// Filter rule for CLI output noise.
///
/// Matches lines that start with a prefix AND contain all required keywords.
/// This is more precise than keyword-only matching to avoid filtering user errors.
struct FilterRule {
    /// Line must start with one of these (case-insensitive, after trim).
    starts_with: &'static [&'static str],
    /// Line must also contain ALL of these keywords (case-insensitive).
    contains: &'static [&'static str],
}

impl FilterRule {
    const fn new(starts_with: &'static [&'static str], contains: &'static [&'static str]) -> Self {
        Self { starts_with, contains }
    }

    fn matches(&self, line: &str) -> bool {
        let lower = line.trim().to_ascii_lowercase();
        // Must start with one of the prefixes
        let has_prefix = self.starts_with.is_empty()
            || self.starts_with.iter().any(|p| lower.starts_with(p));
        // Must contain all keywords
        let has_keywords = self.contains.iter().all(|kw| lower.contains(kw));
        has_prefix && has_keywords
    }
}

/// Output filter configuration.
struct OutputFilter {
    /// Lines matching any rule are filtered out.
    line_rules: &'static [FilterRule],
    /// Prefixes indicating non-output (HTML, JSON).
    skip_prefixes: &'static [&'static str],
}

impl OutputFilter {
    const STDOUT: Self = Self {
        line_rules: &[],
        skip_prefixes: &["<!DOCTYPE", "{"],
    };

    // Typst warning example:
    //   warning: html export is under active development and incomplete
    //    = hint: its behaviour may change at any time
    //   warning: elem `xxx` was ignored during html export
    //
    // Tailwindcss example:
    //   ≈ tailwindcss v4.0.0
    const STDERR: Self = Self {
        line_rules: &[
            // Typst warnings about experimental features
            FilterRule::new(&["warning:"], &["html export"]),
            FilterRule::new(&["warning:"], &["was ignored during", "export"]),
            // Typst hints (always start with "= hint:")
            FilterRule::new(&["= hint:"], &[]),
            // Tailwindcss version banner (starts with ≈ or ~)
            FilterRule::new(&["≈", "~"], &["tailwindcss"]),
        ],
        skip_prefixes: &[],
    };

    /// Check if entire output block should be skipped.
    fn should_skip(&self, output: &str) -> bool {
        output.is_empty() || self.skip_prefixes.iter().any(|p| output.starts_with(p))
    }

    /// Check if a line should be filtered.
    fn should_filter_line(&self, line: &str) -> bool {
        self.line_rules.iter().any(|r| r.matches(line))
    }

    /// Log non-filtered lines.
    fn log(&self, name: &str, output: &str) {
        if self.should_skip(output) {
            return;
        }
        for line in output.lines() {
            if !line.trim().is_empty() && !self.should_filter_line(line) {
                log!(name; "{line}");
            }
        }
    }

    /// Extract error message, skipping filtered lines at start.
    fn extract_error<'a>(&self, stderr: &'a str) -> &'a str {
        stderr
            .lines()
            .find(|line| !line.trim().is_empty() && !self.should_filter_line(line))
            .map(|first| {
                let offset = first.as_ptr() as usize - stderr.as_ptr() as usize;
                &stderr[offset..]
            })
            .unwrap_or(stderr)
            .trim()
    }
}

/// Log command output, filtering known noise.
fn log_output(name: &str, output: &Output) -> Result<()> {
    let stdout = std::str::from_utf8(&output.stdout)
        .context("Invalid UTF-8 in stdout")?
        .trim();
    let stderr = std::str::from_utf8(&output.stderr)
        .context("Invalid UTF-8 in stderr")?
        .trim();

    if !output.status.success() {
        let error_msg = OutputFilter::STDERR.extract_error(stderr);
        if !error_msg.is_empty() {
            eprintln!("{error_msg}");
        }
        anyhow::bail!("Command `{name}` failed with {}", output.status);
    }

    OutputFilter::STDOUT.log(name, stdout);
    OutputFilter::STDERR.log(name, stderr);

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_os() {
        assert_eq!(to_os("hello"), OsString::from("hello"));
        assert_eq!(to_os(String::from("world")), OsString::from("world"));
    }

    #[test]
    fn test_to_cmd_vec_array() {
        let cmd = to_cmd_vec(["git", "status"]);
        assert_eq!(cmd.len(), 2);
        assert_eq!(cmd[0], OsString::from("git"));
        assert_eq!(cmd[1], OsString::from("status"));
    }

    #[test]
    fn test_to_cmd_vec_vec() {
        let v = vec!["echo".to_string(), "hello".to_string()];
        let cmd = to_cmd_vec(&v);
        assert_eq!(cmd.len(), 2);
        assert_eq!(cmd[0], OsString::from("echo"));
        assert_eq!(cmd[1], OsString::from("hello"));
    }

    #[test]
    fn test_filter_args() {
        let args = [OsString::from("a"), OsString::from(""), OsString::from("b")];
        let filtered = filter_args(&args);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0], OsString::from("a"));
        assert_eq!(filtered[1], OsString::from("b"));
    }

    #[test]
    fn test_prepare_empty() {
        let result = prepare(None, &[], &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_prepare_valid() {
        let cmd = to_cmd_vec(["echo"]);
        let args = filter_args(&[OsString::from("hello")]);
        let result = prepare(None, &cmd, &args);
        assert!(result.is_ok());
        let (name, _) = result.unwrap();
        assert_eq!(name, "echo");
    }
}
