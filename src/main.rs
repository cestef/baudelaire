use clap::Parser;

fn main() -> miette::Result<()> {
    let cli = baudelaire::cli::Cli::parse();
    baudelaire::cli::run(cli).map_err(miette::Report::from)
}
