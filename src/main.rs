use std::process::ExitCode;

use baudelaire::ui::{Level, Ui};
use clap::Parser;

fn main() -> ExitCode {
    let cli = baudelaire::cli::Cli::parse();
    match baudelaire::cli::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        // Rendered here rather than by miette's global hook, so a failure comes
        // out through the same reporter, at the same width, with the same markup
        // as every warning above it.
        Err(error) => {
            Ui::new(Level::Default).fail(&error);
            ExitCode::FAILURE
        }
    }
}
