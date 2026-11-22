//! Subcommand implementations. Each command writes its output to the writer
//! it is given, so tests can capture it.

pub(crate) mod completions;
pub(crate) mod serve;
// +scaffold:command:module

/// Settings that shape how every command writes output.
#[derive(Debug, Default)]
#[expect(
    clippy::empty_structs_with_brackets,
    reason = "commands build it with `Context::default()`, which keeps working once fields exist"
)]
pub(crate) struct Context {}
