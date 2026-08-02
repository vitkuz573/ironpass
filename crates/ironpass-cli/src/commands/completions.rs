use crate::args::Cli;
use clap::CommandFactory;
use clap_complete::{Shell, generate};
use std::io;

pub fn handle(shell: Shell) -> color_eyre::eyre::Result<()> {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "ironpass", &mut io::stdout());
    Ok(())
}
