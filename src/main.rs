use clap::Parser;

use autospec::cli::CliArgs;
use autospec::config::RuntimeConfig;

fn main() {
    let cli = CliArgs::parse();
    let result = RuntimeConfig::from_cli(cli).and_then(autospec::engine::run);

    if let Err(error) = result {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}
