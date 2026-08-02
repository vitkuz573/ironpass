use clap::CommandFactory;
use clap_complete::{generate, Shell};
use std::io;
use crate::args::Cli;

pub fn handle(shell: Shell) -> color_eyre::eyre::Result<()> {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "ironpass", &mut io::stdout());
    Ok(())
}
