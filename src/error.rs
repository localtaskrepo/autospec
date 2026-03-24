use std::io;
use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, AutospecError>;

#[derive(Debug, Error)]
pub enum AutospecError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("target path does not exist: {0}")]
    MissingTarget(PathBuf),

    #[error("target path must stay inside the current repository root: {0}")]
    TargetOutsideRepo(PathBuf),

    #[error("no documents found: {0}")]
    EmptyScope(String),

    #[error(
        "no supported built-in agent CLI found; install copilot, claude, codex, or gemini, or use --agent custom --agent-cmd"
    )]
    NoSupportedAgent,

    #[error("requested built-in agent is not installed: {0}")]
    MissingBuiltInAgent(&'static str),

    #[error("invalid custom command template: {0}")]
    InvalidCustomCommand(String),

    #[error("command failed to start: {command}: {source}")]
    SpawnFailed { command: String, source: io::Error },

    #[error("I/O error while {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },

    #[error("git command failed ({args}): {detail}")]
    GitFailed { args: String, detail: String },

    #[error("in-scope docs already have uncommitted changes: {0}")]
    DirtyDocs(String),
}

impl AutospecError {
    pub fn io(action: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }
}
