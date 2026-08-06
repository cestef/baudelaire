use std::process::ExitCode;

use baudelaire::ui::{Level, Ui};

fn main() -> ExitCode {
    // `parsed`, not clap's `parse`: it pre-scans argv for `--color` and applies
    // the choice before clap can write `--help` and exit, so the one flag that
    // has to affect clap's own output is not read too late to.
    match baudelaire::cli::Cli::parsed().run() {
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
