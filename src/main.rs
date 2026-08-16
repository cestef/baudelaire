use std::process::ExitCode;

use baudelaire::ui::{Level, Ui};

fn main() -> ExitCode {
    match baudelaire::cli::Cli::parsed().run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            Ui::new(Level::Default).fail(&error);
            ExitCode::FAILURE
        }
    }
}
