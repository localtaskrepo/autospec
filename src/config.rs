use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::cli::{AgentArg, CliArgs, ScopeArg};
use crate::error::{AutospecError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeMode {
    Strict,
    Ripple,
    Sweep,
}

impl ScopeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ScopeMode::Strict => "strict",
            ScopeMode::Ripple => "ripple",
            ScopeMode::Sweep => "sweep",
        }
    }
}

impl fmt::Display for ScopeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltInAgent {
    Copilot,
    Claude,
    Codex,
    Gemini,
}

impl BuiltInAgent {
    pub fn executable_name(self) -> &'static str {
        match self {
            BuiltInAgent::Copilot => "copilot",
            BuiltInAgent::Claude => "claude",
            BuiltInAgent::Codex => "codex",
            BuiltInAgent::Gemini => "gemini",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.executable_name()
    }
}

impl fmt::Display for BuiltInAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRequest {
    Auto,
    BuiltIn(BuiltInAgent),
    Custom,
}

#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub repo_root: PathBuf,
    pub target: PathBuf,
    pub scope: ScopeMode,
    pub goal: String,
    pub max_iters: u32,
    pub threshold: usize,
    pub stable_iters: u32,
    pub agent_request: AgentRequest,
    pub agent_cmd_template: Option<String>,
    pub model: String,
    pub effort: String,
    pub agent_timeout: Option<Duration>,
    pub skip_readmes: bool,
    pub allow_dirty: bool,
    pub no_commit: bool,
    pub no_branch: bool,
    pub dry_run: bool,
    pub no_artifacts: bool,
    pub max_scope_files: Option<usize>,
}

impl RuntimeConfig {
    pub fn from_cli(cli: CliArgs) -> Result<Self> {
        let CliArgs {
            target,
            agent,
            agent_cmd,
            model,
            effort,
            agent_timeout,
            max_iters,
            threshold,
            stable_iters,
            scope,
            max_scope_files,
            goal,
            doc_dir,
            skip_readmes,
            allow_dirty,
            no_commit,
            no_branch,
            dry_run,
            no_artifacts,
        } = cli;

        let repo_root = env::current_dir()
            .map_err(|source| AutospecError::io("reading current directory", ".", source))?;
        let repo_root = repo_root.canonicalize().map_err(|source| {
            AutospecError::io("canonicalizing current directory", &repo_root, source)
        })?;

        let scope = resolve_scope(scope)?;
        let target_input = target
            .or(doc_dir)
            .or_else(|| read_env_path("DOC_DIR"))
            .unwrap_or_else(|| PathBuf::from("docs"));

        let target = resolve_target(&repo_root, &target_input)?;

        let model = resolve_string(model, "MODEL", "gpt-5.4");
        let effort = resolve_string(effort, "EFFORT", "");
        let goal = resolve_string(goal, "GOAL", "");
        let max_iters = resolve_u32(max_iters, "MAX_ITERS", 10, 1)?;
        let threshold = resolve_usize(threshold, "THRESHOLD", 10, 0)?;
        let stable_iters = resolve_u32(stable_iters, "STABLE_ITERS", 2, 1)?;
        let max_scope_files = resolve_optional_usize(max_scope_files, "MAX_SCOPE_FILES")?;
        let max_scope_files = max_scope_files.filter(|value| *value > 0);
        let agent_timeout = resolve_optional_u64(agent_timeout, "AGENT_TIMEOUT")?
            .map(Duration::from_secs)
            .filter(|duration| !duration.is_zero());

        let skip_readmes = resolve_flag(skip_readmes, "SKIP_READMES");
        let allow_dirty = resolve_flag(allow_dirty, "ALLOW_DIRTY");
        let no_commit = resolve_flag(no_commit, "NO_COMMIT");
        let no_branch = resolve_flag(no_branch, "NO_BRANCH");
        let dry_run = resolve_flag(dry_run, "DRY_RUN");
        let no_artifacts = resolve_flag(no_artifacts, "NO_ARTIFACTS");

        let agent_request = resolve_agent_request(agent)?;

        let agent_cmd_template = if matches!(agent_request, AgentRequest::Custom) {
            agent_cmd
                .or_else(|| env::var("AGENT_CMD").ok())
                .filter(|value| !value.trim().is_empty())
        } else {
            None
        };

        if matches!(agent_request, AgentRequest::Custom) && agent_cmd_template.is_none() {
            return Err(AutospecError::InvalidConfig(
                "--agent custom requires --agent-cmd or AGENT_CMD".to_owned(),
            ));
        }

        Ok(Self {
            repo_root,
            target,
            scope,
            goal,
            max_iters,
            threshold,
            stable_iters,
            agent_request,
            agent_cmd_template,
            model,
            effort,
            agent_timeout,
            skip_readmes,
            allow_dirty,
            no_commit,
            no_branch,
            dry_run,
            no_artifacts,
            max_scope_files,
        })
    }
}

fn resolve_scope(cli_scope: Option<ScopeArg>) -> Result<ScopeMode> {
    if let Some(scope) = cli_scope {
        Ok(scope.into())
    } else if let Some(scope) = read_env_scope("SCOPE")? {
        Ok(scope)
    } else {
        Ok(ScopeMode::Strict)
    }
}

fn resolve_agent_request(cli_agent: Option<AgentArg>) -> Result<AgentRequest> {
    if let Some(agent) = cli_agent {
        Ok(parse_agent_arg(agent))
    } else if let Some(agent) = read_env_agent("AGENT")? {
        Ok(agent)
    } else {
        Ok(AgentRequest::Auto)
    }
}

fn parse_agent_arg(agent: AgentArg) -> AgentRequest {
    match agent {
        AgentArg::Copilot => AgentRequest::BuiltIn(BuiltInAgent::Copilot),
        AgentArg::Claude => AgentRequest::BuiltIn(BuiltInAgent::Claude),
        AgentArg::Codex => AgentRequest::BuiltIn(BuiltInAgent::Codex),
        AgentArg::Gemini => AgentRequest::BuiltIn(BuiltInAgent::Gemini),
        AgentArg::Custom => AgentRequest::Custom,
    }
}

fn read_env_agent(name: &str) -> Result<Option<AgentRequest>> {
    let Some(raw) = env::var(name).ok() else {
        return Ok(None);
    };

    parse_agent_request_value(&raw, name).map(Some)
}

fn parse_agent_request_value(raw: &str, source_name: &str) -> Result<AgentRequest> {
    let request = match raw {
        "copilot" => AgentRequest::BuiltIn(BuiltInAgent::Copilot),
        "claude" => AgentRequest::BuiltIn(BuiltInAgent::Claude),
        "codex" => AgentRequest::BuiltIn(BuiltInAgent::Codex),
        "gemini" => AgentRequest::BuiltIn(BuiltInAgent::Gemini),
        "custom" => AgentRequest::Custom,
        _ => {
            return Err(AutospecError::InvalidConfig(format!(
                "{source_name} must be one of copilot|claude|codex|gemini|custom"
            )));
        }
    };

    Ok(request)
}

fn read_env_scope(name: &str) -> Result<Option<ScopeMode>> {
    let Some(raw) = env::var(name).ok() else {
        return Ok(None);
    };

    parse_scope_value(&raw, name).map(Some)
}

fn parse_scope_value(raw: &str, source_name: &str) -> Result<ScopeMode> {
    let scope = match raw {
        "strict" => ScopeMode::Strict,
        "ripple" => ScopeMode::Ripple,
        "sweep" => ScopeMode::Sweep,
        _ => {
            return Err(AutospecError::InvalidConfig(format!(
                "{source_name} must be one of strict|ripple|sweep"
            )));
        }
    };

    Ok(scope)
}

fn read_env_flag(name: &str) -> bool {
    matches!(env::var(name).ok().as_deref(), Some("1"))
}

fn resolve_flag(cli_value: bool, env_name: &str) -> bool {
    cli_value || read_env_flag(env_name)
}

fn read_env_path(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
}

fn resolve_string(cli: Option<String>, env_name: &str, default: &str) -> String {
    cli.or_else(|| env::var(env_name).ok())
        .unwrap_or_else(|| default.to_owned())
}

fn resolve_u32(cli: Option<u32>, env_name: &str, default: u32, min: u32) -> Result<u32> {
    let value = if let Some(value) = cli {
        value
    } else if let Ok(raw) = env::var(env_name) {
        raw.parse::<u32>().map_err(|_| {
            AutospecError::InvalidConfig(format!("{env_name} must be a base-10 integer"))
        })?
    } else {
        default
    };

    if value < min {
        return Err(AutospecError::InvalidConfig(format!(
            "{env_name} / CLI value must be >= {min}"
        )));
    }

    Ok(value)
}

fn resolve_usize(cli: Option<usize>, env_name: &str, default: usize, min: usize) -> Result<usize> {
    let value = if let Some(value) = cli {
        value
    } else if let Ok(raw) = env::var(env_name) {
        raw.parse::<usize>().map_err(|_| {
            AutospecError::InvalidConfig(format!("{env_name} must be a base-10 integer"))
        })?
    } else {
        default
    };

    if value < min {
        return Err(AutospecError::InvalidConfig(format!(
            "{env_name} / CLI value must be >= {min}"
        )));
    }

    Ok(value)
}

fn resolve_optional_usize(cli: Option<usize>, env_name: &str) -> Result<Option<usize>> {
    if let Some(value) = cli {
        return Ok(Some(value));
    }

    let Some(raw) = env::var(env_name).ok() else {
        return Ok(None);
    };

    let value = raw.parse::<usize>().map_err(|_| {
        AutospecError::InvalidConfig(format!("{env_name} must be a base-10 integer"))
    })?;
    Ok(Some(value))
}

fn resolve_optional_u64(cli: Option<u64>, env_name: &str) -> Result<Option<u64>> {
    if let Some(value) = cli {
        return Ok(Some(value));
    }

    let Some(raw) = env::var(env_name).ok() else {
        return Ok(None);
    };

    let value = raw.parse::<u64>().map_err(|_| {
        AutospecError::InvalidConfig(format!("{env_name} must be a base-10 integer"))
    })?;
    Ok(Some(value))
}

fn resolve_target(repo_root: &Path, raw_target: &Path) -> Result<PathBuf> {
    let resolved = if raw_target.is_absolute() {
        raw_target.to_path_buf()
    } else {
        repo_root.join(raw_target)
    };

    if !resolved.exists() {
        return Err(AutospecError::MissingTarget(resolved));
    }

    let canonical = resolved
        .canonicalize()
        .map_err(|source| AutospecError::io("canonicalizing target", &resolved, source))?;

    if !canonical.starts_with(repo_root) {
        return Err(AutospecError::TargetOutsideRepo(canonical));
    }

    Ok(canonical)
}

impl From<ScopeArg> for ScopeMode {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Strict => ScopeMode::Strict,
            ScopeArg::Ripple => ScopeMode::Ripple,
            ScopeArg::Sweep => ScopeMode::Sweep,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_agent_request_value_accepts_custom() {
        let parsed = parse_agent_request_value("custom", "AGENT").unwrap();
        assert_eq!(parsed, AgentRequest::Custom);
    }

    #[test]
    fn parse_agent_request_value_rejects_unknown_values() {
        let error = parse_agent_request_value("invalid", "AGENT").unwrap_err();
        assert!(
            matches!(error, AutospecError::InvalidConfig(message) if message == "AGENT must be one of copilot|claude|codex|gemini|custom")
        );
    }

    #[test]
    fn parse_scope_value_accepts_ripple() {
        let parsed = parse_scope_value("ripple", "SCOPE").unwrap();
        assert_eq!(parsed, ScopeMode::Ripple);
    }

    #[test]
    fn parse_scope_value_rejects_unknown_values() {
        let error = parse_scope_value("invalid", "SCOPE").unwrap_err();
        assert!(
            matches!(error, AutospecError::InvalidConfig(message) if message == "SCOPE must be one of strict|ripple|sweep")
        );
    }

    #[test]
    fn resolve_flag_prefers_cli_true() {
        assert!(resolve_flag(true, "UNUSED_ENV_FLAG"));
    }
}
