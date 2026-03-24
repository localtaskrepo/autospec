use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    Strict,
    Ripple,
    Sweep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentArg {
    Copilot,
    Claude,
    Codex,
    Gemini,
    Custom,
}

#[derive(Debug, Parser)]
#[command(
    name = "autospec",
    about = "Converge markdown specifications using local AI coding agent CLIs",
    long_about = None,
    version
)]
pub struct CliArgs {
    #[arg(value_name = "target")]
    pub target: Option<PathBuf>,

    #[arg(long, value_enum)]
    pub agent: Option<AgentArg>,

    #[arg(long)]
    pub agent_cmd: Option<String>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub effort: Option<String>,

    #[arg(long)]
    pub agent_timeout: Option<u64>,

    #[arg(long)]
    pub max_iters: Option<u32>,

    #[arg(long)]
    pub threshold: Option<usize>,

    #[arg(long)]
    pub stable_iters: Option<u32>,

    #[arg(long, value_enum)]
    pub scope: Option<ScopeArg>,

    #[arg(long)]
    pub max_scope_files: Option<usize>,

    #[arg(long)]
    pub goal: Option<String>,

    #[arg(long)]
    pub doc_dir: Option<PathBuf>,

    #[arg(long)]
    pub skip_readmes: bool,

    #[arg(long)]
    pub allow_dirty: bool,

    #[arg(long)]
    pub no_commit: bool,

    #[arg(long)]
    pub no_branch: bool,

    #[arg(long)]
    pub dry_run: bool,
}
