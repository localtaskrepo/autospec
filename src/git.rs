use std::path::Path;
use std::process::Command;

use crate::error::{AutospecError, Result};

pub fn create_branch(repo_root: &Path, name: &str) -> Result<()> {
    let output = git(repo_root, &["checkout", "-b", name])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AutospecError::GitFailed {
            args: format!("checkout -b {name}"),
            detail: stderr_or_stdout(&output),
        })
    }
}

pub fn dirty_docs(repo_root: &Path, docs: &[String]) -> Result<Vec<String>> {
    let mut dirty = Vec::new();
    for doc in docs {
        let output = git(repo_root, &["diff", "--quiet", "--", doc.as_str()])?;
        match output.status.code() {
            Some(0) => {}
            Some(1) => dirty.push(doc.clone()),
            _ => {
                return Err(AutospecError::GitFailed {
                    args: format!("diff --quiet -- {doc}"),
                    detail: stderr_or_stdout(&output),
                });
            }
        }
    }

    Ok(dirty)
}

pub fn stage_and_commit_changes(
    repo_root: &Path,
    changed_files: &[String],
    message: &str,
) -> Result<()> {
    if changed_files.is_empty() {
        return Err(AutospecError::GitFailed {
            args: "add/commit".to_owned(),
            detail: "files changed but nothing was staged".to_owned(),
        });
    }

    let mut add_args = Vec::with_capacity(changed_files.len() + 1);
    add_args.push("add");
    add_args.extend(changed_files.iter().map(String::as_str));
    let add = git(repo_root, &add_args)?;
    if !add.status.success() {
        return Err(AutospecError::GitFailed {
            args: format!("add {}", changed_files.join(" ")),
            detail: stderr_or_stdout(&add),
        });
    }

    let staged = git(repo_root, &["diff", "--cached", "--quiet"])?;
    if matches!(staged.status.code(), Some(0)) {
        return Err(AutospecError::GitFailed {
            args: "diff --cached --quiet".to_owned(),
            detail: "files changed but nothing was staged".to_owned(),
        });
    }

    let commit = git(repo_root, &["commit", "-m", message, "--no-verify"])?;
    if commit.status.success() {
        Ok(())
    } else {
        Err(AutospecError::GitFailed {
            args: format!("commit -m {message}"),
            detail: stderr_or_stdout(&commit),
        })
    }
}

fn git(repo_root: &Path, args: &[&str]) -> Result<std::process::Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|source| AutospecError::SpawnFailed {
            command: format!("git {}", args.join(" ")),
            source,
        })
}

fn stderr_or_stdout(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !stdout.is_empty() {
        return stdout;
    }

    "git command failed".to_owned()
}
