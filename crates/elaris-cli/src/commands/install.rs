use std::path::Path;

use crate::support::{command, stage, success};

pub(crate) fn install(root: &Path, verbose: bool) -> Result<(), String> {
    stage("Install the Elaris CLI in Cargo's bin directory");
    command(
        "cargo",
        &[
            "install",
            "--path",
            "crates/elaris-cli",
            "--locked",
            "--force",
        ],
        root,
        verbose,
    )?;
    success("Elaris CLI is installed; run `elaris` or `el`");
    Ok(())
}
