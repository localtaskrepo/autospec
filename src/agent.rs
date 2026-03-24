use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

use crate::config::{AgentRequest, BuiltInAgent, RuntimeConfig};
use crate::error::{AutospecError, Result};

const BUILTIN_AGENTS: [BuiltInAgent; 4] = [
    BuiltInAgent::Copilot,
    BuiltInAgent::Claude,
    BuiltInAgent::Codex,
    BuiltInAgent::Gemini,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAvailability {
    pub kind: BuiltInAgent,
    pub executable: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ResolvedAgent {
    BuiltIn {
        kind: BuiltInAgent,
        executable: PathBuf,
    },
    Custom {
        template: String,
    },
}

#[derive(Debug)]
pub struct AgentRunRequest {
    pub prompt: String,
    pub log_path: PathBuf,
    pub model: String,
    pub effort: String,
    pub timeout: Option<Duration>,
    pub cwd: PathBuf,
    pub dry_run: bool,
}

#[derive(Debug)]
pub enum AgentRunResult {
    Completed,
    TimedOut,
    Failed,
}

pub fn resolve_agent(config: &RuntimeConfig) -> Result<ResolvedAgent> {
    match &config.agent_request {
        AgentRequest::Auto => {
            let (kind, executable) = select_builtin_agent(None, find_executable)?;
            Ok(ResolvedAgent::BuiltIn { kind, executable })
        }
        AgentRequest::BuiltIn(kind) => {
            let (kind, executable) = select_builtin_agent(Some(*kind), find_executable)?;
            Ok(ResolvedAgent::BuiltIn { kind, executable })
        }
        AgentRequest::Custom => Ok(ResolvedAgent::Custom {
            template: config.agent_cmd_template.clone().unwrap_or_default(),
        }),
    }
}

pub fn select_builtin_agent<F>(
    requested: Option<BuiltInAgent>,
    locator: F,
) -> Result<(BuiltInAgent, PathBuf)>
where
    F: Fn(&str) -> Option<PathBuf>,
{
    if let Some(kind) = requested {
        return locator(kind.executable_name())
            .map(|path| (kind, path))
            .ok_or(AutospecError::MissingBuiltInAgent(kind.executable_name()));
    }

    for kind in BUILTIN_AGENTS {
        if let Some(path) = locator(kind.executable_name()) {
            return Ok((kind, path));
        }
    }

    Err(AutospecError::NoSupportedAgent)
}

pub fn list_builtin_availability() -> Vec<AgentAvailability> {
    BUILTIN_AGENTS
        .into_iter()
        .map(|kind| AgentAvailability {
            kind,
            executable: find_executable(kind.executable_name()),
        })
        .collect()
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        return resolve_candidate(candidate);
    }

    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if let Some(path) = resolve_candidate(&candidate) {
            return Some(path);
        }
    }
    None
}

fn resolve_candidate(path: &Path) -> Option<PathBuf> {
    if is_executable(path) {
        return Some(path.to_path_buf());
    }

    #[cfg(windows)]
    {
        let extensions = std::env::var_os("PATHEXT")?;
        for extension in std::env::split_paths(&extensions) {
            let suffix = extension.to_string_lossy();
            let candidate = PathBuf::from(format!("{}{}", path.display(), suffix));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }

    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    match fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

impl ResolvedAgent {
    pub fn display_name(&self) -> &'static str {
        match self {
            ResolvedAgent::BuiltIn { kind, .. } => kind.as_str(),
            ResolvedAgent::Custom { .. } => "custom",
        }
    }
}

pub fn run_agent(agent: &ResolvedAgent, request: &AgentRunRequest) -> Result<AgentRunResult> {
    if request.dry_run {
        println!(
            "  [dry-run] {} --model {}",
            agent.display_name(),
            request.model
        );
        return Ok(AgentRunResult::Completed);
    }

    ensure_log_parent_dir(&request.log_path)?;

    let prepared = prepare_command(agent, request)?;
    let command_name = prepared.program.to_string_lossy().into_owned();

    let mut command = Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(&request.cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if prepared.stdin.is_some() {
        command.stdin(Stdio::piped());
    }

    let mut child = command
        .spawn()
        .map_err(|source| AutospecError::SpawnFailed {
            command: command_name,
            source,
        })?;

    if let Some(stdin_input) = prepared.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(stdin_input.as_bytes()).map_err(|source| {
            AutospecError::io("writing agent stdin", &request.log_path, source)
        })?;
    }

    let output = if let Some(timeout) = request.timeout {
        match child
            .wait_timeout(timeout)
            .map_err(|source| AutospecError::io("waiting for agent", &request.log_path, source))?
        {
            Some(_) => child.wait_with_output().map_err(|source| {
                AutospecError::io("reading agent output", &request.log_path, source)
            })?,
            None => {
                let _ = child.kill();
                let output = child.wait_with_output().map_err(|source| {
                    AutospecError::io("reading timed-out agent output", &request.log_path, source)
                })?;
                maybe_write_log(
                    &request.log_path,
                    &output.stdout,
                    &output.stderr,
                    prepared.agent_writes_log,
                )?;
                print_tail(&output.stdout, &output.stderr);
                return Ok(AgentRunResult::TimedOut);
            }
        }
    } else {
        child.wait_with_output().map_err(|source| {
            AutospecError::io("reading agent output", &request.log_path, source)
        })?
    };

    maybe_write_log(
        &request.log_path,
        &output.stdout,
        &output.stderr,
        prepared.agent_writes_log,
    )?;
    print_tail(&output.stdout, &output.stderr);

    if output.status.success() {
        Ok(AgentRunResult::Completed)
    } else {
        Ok(AgentRunResult::Failed)
    }
}

struct PreparedCommand {
    program: PathBuf,
    args: Vec<String>,
    stdin: Option<String>,
    agent_writes_log: bool,
}

fn prepare_command(agent: &ResolvedAgent, request: &AgentRunRequest) -> Result<PreparedCommand> {
    match agent {
        ResolvedAgent::BuiltIn { kind, executable } => {
            prepare_builtin_command(*kind, executable.clone(), request)
        }
        ResolvedAgent::Custom { template } => prepare_custom_command(template, request),
    }
}

fn prepare_builtin_command(
    kind: BuiltInAgent,
    executable: PathBuf,
    request: &AgentRunRequest,
) -> Result<PreparedCommand> {
    let log_path = request.log_path.to_string_lossy().into_owned();
    let mut args = Vec::new();
    let mut stdin = None;
    let mut agent_writes_log = false;

    match kind {
        BuiltInAgent::Copilot => {
            args.extend([
                "-p".to_owned(),
                request.prompt.clone(),
                "--yolo".to_owned(),
                "--model".to_owned(),
                request.model.clone(),
                format!("--share={log_path}"),
                "--no-alt-screen".to_owned(),
            ]);
            if !request.effort.is_empty() {
                args.push("--effort".to_owned());
                args.push(request.effort.clone());
            }
            agent_writes_log = true;
        }
        BuiltInAgent::Claude => {
            args.extend([
                "-p".to_owned(),
                request.prompt.clone(),
                "--dangerously-skip-permissions".to_owned(),
                "--model".to_owned(),
                request.model.clone(),
                "--output-format".to_owned(),
                "text".to_owned(),
                "--no-session-persistence".to_owned(),
            ]);
            if !request.effort.is_empty() {
                args.push("--effort".to_owned());
                args.push(request.effort.clone());
            }
        }
        BuiltInAgent::Codex => {
            args.extend([
                "exec".to_owned(),
                "--full-auto".to_owned(),
                "-m".to_owned(),
                request.model.clone(),
                "-C".to_owned(),
                request.cwd.to_string_lossy().into_owned(),
                "-o".to_owned(),
                log_path,
                "-".to_owned(),
            ]);
            stdin = Some(request.prompt.clone());
            agent_writes_log = true;
        }
        BuiltInAgent::Gemini => {
            args.extend([
                "-p".to_owned(),
                request.prompt.clone(),
                "-y".to_owned(),
                "-m".to_owned(),
                request.model.clone(),
                "--output-format".to_owned(),
                "text".to_owned(),
            ]);
        }
    }

    Ok(PreparedCommand {
        program: executable,
        args,
        stdin,
        agent_writes_log,
    })
}

fn prepare_custom_command(template: &str, request: &AgentRunRequest) -> Result<PreparedCommand> {
    let tokens = shell_words::split(template)
        .map_err(|error| AutospecError::InvalidCustomCommand(error.to_string()))?;
    if tokens.is_empty() {
        return Err(AutospecError::InvalidCustomCommand(
            "template must not be empty".to_owned(),
        ));
    }

    let log_path = request.log_path.to_string_lossy().into_owned();
    let cwd = request.cwd.to_string_lossy().into_owned();
    let mut tokens = tokens
        .into_iter()
        .map(|token| {
            token
                .replace("{prompt}", &request.prompt)
                .replace("{model}", &request.model)
                .replace("{effort}", &request.effort)
                .replace("{log}", &log_path)
                .replace("{cwd}", &cwd)
        })
        .collect::<Vec<_>>();

    let program = PathBuf::from(tokens.remove(0));
    Ok(PreparedCommand {
        program,
        args: tokens,
        stdin: None,
        agent_writes_log: false,
    })
}

fn maybe_write_log(
    log_path: &Path,
    stdout: &[u8],
    stderr: &[u8],
    agent_writes_log: bool,
) -> Result<()> {
    if agent_writes_log && log_has_content(log_path) {
        return Ok(());
    }

    let content = combined_output(stdout, stderr);

    if content.is_empty() {
        if log_path.exists() && !log_has_content(log_path) {
            let _ = fs::remove_file(log_path);
        }
        return Ok(());
    }

    fs::write(log_path, content)
        .map_err(|source| AutospecError::io("writing agent log", log_path, source))
}

fn ensure_log_parent_dir(log_path: &Path) -> Result<()> {
    let Some(parent) = log_path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent)
        .map_err(|source| AutospecError::io("creating agent log directory", parent, source))
}

fn log_has_content(log_path: &Path) -> bool {
    fs::metadata(log_path)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

fn combined_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut content = String::new();
    if !stdout.is_empty() {
        content.push_str(&String::from_utf8_lossy(stdout));
    }
    if !stderr.is_empty() {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&String::from_utf8_lossy(stderr));
    }
    content
}

fn print_tail(stdout: &[u8], stderr: &[u8]) {
    let mut lines = String::new();
    lines.push_str(&String::from_utf8_lossy(stdout));
    lines.push_str(&String::from_utf8_lossy(stderr));
    let tail = lines
        .lines()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    for line in tail {
        println!("  {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::BuiltInAgent;
    use tempfile::tempdir;

    #[test]
    fn auto_selects_priority_order() {
        let resolved = select_builtin_agent(None, |name| match name {
            "copilot" => None,
            "claude" => Some(PathBuf::from("/tmp/claude")),
            "codex" => Some(PathBuf::from("/tmp/codex")),
            _ => None,
        })
        .unwrap();

        assert_eq!(resolved.0, BuiltInAgent::Claude);
    }

    #[test]
    fn explicit_builtin_does_not_fall_back() {
        let error = select_builtin_agent(Some(BuiltInAgent::Gemini), |_| None).unwrap_err();
        assert!(matches!(
            error,
            AutospecError::MissingBuiltInAgent("gemini")
        ));
    }

    #[test]
    fn falls_back_to_captured_output_when_agent_log_is_empty() {
        let temp = tempdir().unwrap();
        let log_path = temp.path().join("agent.log");
        fs::write(&log_path, "").unwrap();

        maybe_write_log(&log_path, b"stdout line\n", b"", true).unwrap();

        assert_eq!(fs::read_to_string(&log_path).unwrap(), "stdout line\n");
    }
}
