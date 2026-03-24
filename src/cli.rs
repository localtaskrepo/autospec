use std::path::PathBuf;

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    #[value(help = "Only edit the target doc")]
    Strict,
    #[value(help = "Focus on one doc but allow related files in scope to change")]
    Ripple,
    #[value(help = "Review and refine a whole docs directory as one scope")]
    Sweep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AgentArg {
    #[value(help = "Use GitHub Copilot CLI")]
    Copilot,
    #[value(help = "Use Claude Code CLI")]
    Claude,
    #[value(help = "Use OpenAI Codex CLI")]
    Codex,
    #[value(help = "Use Gemini CLI")]
    Gemini,
    #[value(help = "Use a custom command template")]
    Custom,
}

#[derive(Debug, Parser)]
#[command(
    name = "autospec",
    about = "Converge markdown specifications using local AI coding agent CLIs",
    long_about = "Run an AI coding agent in a convergence loop against markdown docs.\n\nUse strict mode to refine one file in isolation, ripple mode to allow limited cross-doc fixes in the surrounding scope, or sweep mode to review a whole folder and touch the smallest set of files needed.",
    after_long_help = "Examples:\n  autospec docs/product.md\n  autospec --no-commit --no-artifacts docs/product.md\n  autospec --scope ripple docs/entity-dictionary.md\n  autospec --scope sweep docs/ui\n  autospec --agent custom --agent-cmd 'my-agent --prompt {prompt} --log {log}' docs/product.md\n\nEnvironment overrides:\n  AGENT, AGENT_CMD, MODEL, EFFORT, AGENT_TIMEOUT, MAX_ITERS, THRESHOLD,\n  STABLE_ITERS, SCOPE, MAX_SCOPE_FILES, GOAL, DOC_DIR, SKIP_READMES,\n  ALLOW_DIRTY, NO_COMMIT, NO_BRANCH, DRY_RUN, NO_ARTIFACTS\n\nNotes:\n  --agent-timeout 0 disables the timeout.\n  --no-artifacts avoids creating a repo-local .autospec/ directory.",
    version
)]
pub struct CliArgs {
    #[arg(
        value_name = "target",
        help = "Markdown file or docs directory to process",
        long_help = "Markdown file or docs directory to process. If omitted, autospec uses --doc-dir, DOC_DIR, or defaults to docs/."
    )]
    pub target: Option<PathBuf>,

    #[arg(
        long,
        value_enum,
        help_heading = "Agent",
        help = "Agent CLI to run",
        long_help = "Agent CLI to run. Use custom together with --agent-cmd to supply your own command template."
    )]
    pub agent: Option<AgentArg>,

    #[arg(
        long,
        help_heading = "Agent",
        help = "Template for a custom agent command",
        long_help = "Template for a custom agent command. Supported placeholders: {prompt}, {model}, {effort}, {log}, {cwd}. Required when --agent custom is used unless AGENT_CMD is set."
    )]
    pub agent_cmd: Option<String>,

    #[arg(
        long,
        help_heading = "Agent",
        help = "Model name passed to the agent",
        long_help = "Model name passed through to the selected agent CLI. Defaults to gpt-5.4 unless MODEL is set."
    )]
    pub model: Option<String>,

    #[arg(
        long,
        help_heading = "Agent",
        help = "Optional reasoning-effort hint",
        long_help = "Optional reasoning-effort hint passed through to agents that support it."
    )]
    pub effort: Option<String>,

    #[arg(
        long,
        help_heading = "Convergence",
        help = "Per-iteration timeout in seconds",
        long_help = "Per-iteration timeout in seconds. Use 0 to disable the timeout entirely."
    )]
    pub agent_timeout: Option<u64>,

    #[arg(
        long,
        help_heading = "Convergence",
        help = "Maximum convergence iterations",
        long_help = "Maximum number of iterations to run before autospec reports not-converged."
    )]
    pub max_iters: Option<u32>,

    #[arg(
        long,
        help_heading = "Convergence",
        help = "Low-delta threshold",
        long_help = "Threshold for considering an iteration a low-delta change. Used with --stable-iters to detect near-convergence."
    )]
    pub threshold: Option<usize>,

    #[arg(
        long,
        help_heading = "Convergence",
        help = "Required consecutive low-delta iterations",
        long_help = "Number of consecutive low-delta iterations needed before autospec reports near-convergence."
    )]
    pub stable_iters: Option<u32>,

    #[arg(
        long,
        value_enum,
        help_heading = "Scope",
        help = "Scope mode: strict, ripple, or sweep",
        long_help = "Scope mode. strict edits only the target doc. ripple focuses on one doc but allows related files in the surrounding scope to change. sweep reviews an entire docs directory and edits the smallest set of files needed."
    )]
    pub scope: Option<ScopeArg>,

    #[arg(
        long,
        help_heading = "Scope",
        help = "Cap the number of files in scope",
        long_help = "Cap the number of markdown files allowed in scope. Useful as a safety rail for broad ripple or sweep runs."
    )]
    pub max_scope_files: Option<usize>,

    #[arg(
        long,
        help_heading = "Convergence",
        help = "Extra goal text for the prompt",
        long_help = "Extra goal text appended to the prompt so you can steer the run toward a specific concern such as consistency, architecture, or permissions."
    )]
    pub goal: Option<String>,

    #[arg(
        long,
        help_heading = "Scope",
        help = "Fallback docs directory",
        long_help = "Fallback docs directory used when no positional target is provided. Equivalent to DOC_DIR."
    )]
    pub doc_dir: Option<PathBuf>,

    #[arg(
        long,
        help_heading = "Scope",
        help = "Exclude README files from scope discovery",
        long_help = "Exclude README.md files from scope discovery when autospec expands a directory target."
    )]
    pub skip_readmes: bool,

    #[arg(
        long,
        help_heading = "Safety And Git",
        help = "Allow dirty docs in scope",
        long_help = "Allow autospec to run even when docs in scope already have uncommitted changes."
    )]
    pub allow_dirty: bool,

    #[arg(
        long,
        help_heading = "Safety And Git",
        help = "Do not create iteration commits",
        long_help = "Do not create iteration commits. autospec will leave changes in the working tree instead."
    )]
    pub no_commit: bool,

    #[arg(
        long,
        help_heading = "Safety And Git",
        help = "Do not create an autospec branch",
        long_help = "Do not create an autospec/<timestamp> branch before starting a run. Has no effect when --no-commit is enabled."
    )]
    pub no_branch: bool,

    #[arg(
        long,
        help_heading = "Output",
        help = "Print the first iteration prompt only",
        long_help = "Print the first iteration prompt and execute the agent request once without entering the convergence loop."
    )]
    pub dry_run: bool,

    #[arg(
        long,
        help_heading = "Output",
        help = "Do not write .autospec artifacts",
        long_help = "Do not write repo-local .autospec artifacts such as results.tsv and run logs. Agent-side log plumbing still uses temporary paths when needed."
    )]
    pub no_artifacts: bool,
}
