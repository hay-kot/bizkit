//! The `openapi` command.

use std::io::Write;

use anyhow::Context as _;

use crate::modules::modules;

#[derive(Debug, clap::Args)]
pub(crate) struct Args;

/// Writes the OpenAPI document. It needs no config or database: the spec
/// comes from module registration alone.
pub(crate) fn run(_args: &Args, out: &mut impl Write) -> anyhow::Result<()> {
    let spec = bizkit_core::openapi(&modules())?;
    let json = serde_json::to_string_pretty(&spec).context("serializing the spec")?;
    writeln!(out, "{json}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_spec_with_the_crate_version() {
        let mut out = Vec::new();
        run(&Args, &mut out).unwrap();
        let spec: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(spec["info"]["version"], env!("CARGO_PKG_VERSION"), "info.version");
        assert!(spec["paths"]["/api/v1/workspaces"].is_object(), "core operations are documented");
    }
}
