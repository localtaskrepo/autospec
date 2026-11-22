use clap::Parser;

use autospec::cli::CliArgs;
use autospec::config::RuntimeConfig;

/// A doc converged but still holds open `[NEEDS OWNER: …]` questions.
const NEEDS_OWNER_EXIT: i32 = 3;

fn main() {
    let cli = CliArgs::parse();
    let result = RuntimeConfig::from_cli(cli).and_then(autospec::engine::run);

    match result {
        Ok(summary) if summary.agent_failures > 0 => {
            eprintln!(
                "ERROR: the agent failed or timed out on {} doc(s); see the logs",
                summary.agent_failures
            );
            std::process::exit(1);
        }
        Ok(summary) if summary.needs_owner() => std::process::exit(NEEDS_OWNER_EXIT),
        Ok(_) => {}
        Err(error) => {
            eprintln!("ERROR: {error}");
            std::process::exit(1);
        }
    }
}
