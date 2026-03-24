use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{AutospecError, Result};

#[derive(Debug, Clone)]
pub struct OutputPaths {
    log_dir: Option<PathBuf>,
    results_file: Option<PathBuf>,
    agent_log_dir: PathBuf,
}

impl OutputPaths {
    pub fn artifacts_enabled(&self) -> bool {
        self.results_file.is_some()
    }

    pub fn log_dir(&self) -> Option<&Path> {
        self.log_dir.as_deref()
    }

    pub fn results_file(&self) -> Option<&Path> {
        self.results_file.as_deref()
    }

    pub fn results_display(&self, dry_run: bool) -> String {
        if dry_run {
            "(not written in dry-run)".to_owned()
        } else if !self.artifacts_enabled() {
            "(disabled via --no-artifacts)".to_owned()
        } else {
            self.results_file
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "(disabled)".to_owned())
        }
    }

    pub fn logs_display(&self, dry_run: bool) -> String {
        if dry_run {
            "(not written in dry-run)".to_owned()
        } else if !self.artifacts_enabled() {
            "(disabled via --no-artifacts)".to_owned()
        } else {
            self.log_dir
                .as_ref()
                .map(|path| format!("{}/", path.display()))
                .unwrap_or_else(|| "(disabled)".to_owned())
        }
    }

    pub fn iteration_log_path(&self, slug: &str, iteration: u32) -> PathBuf {
        self.agent_log_dir
            .join(format!("{}_iter{iteration}.md", slug))
    }
}

pub fn output_paths(repo_root: &Path, no_artifacts: bool) -> OutputPaths {
    if no_artifacts {
        let temp_root = std::env::temp_dir().join(format!(
            "autospec-{}-{}",
            std::process::id(),
            Utc::now().format("%Y%m%dT%H%M%S%f")
        ));
        let agent_log_dir = temp_root.join("logs");
        OutputPaths {
            log_dir: None,
            results_file: None,
            agent_log_dir,
        }
    } else {
        let base_dir = repo_root.join(".autospec");
        let log_dir = base_dir.join("logs");
        let results_file = base_dir.join("results.tsv");
        OutputPaths {
            log_dir: Some(log_dir.clone()),
            results_file: Some(results_file),
            agent_log_dir: log_dir,
        }
    }
}

pub fn append_result(
    results_file: Option<&Path>,
    doc: &str,
    iterations: u32,
    status: &str,
    delta: &str,
) -> Result<()> {
    let Some(results_file) = results_file else {
        return Ok(());
    };

    ensure_results_file(results_file)?;

    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(results_file)
        .map_err(|source| AutospecError::io("opening results file", results_file, source))?;

    let timestamp = Utc::now().to_rfc3339();
    writeln!(
        writer,
        "{doc}\t{iterations}\t{status}\t{delta}\t{timestamp}"
    )
    .map_err(|source| AutospecError::io("appending results file", results_file, source))
}

pub fn append_log(path: Option<&Path>, line: &str) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    ensure_parent_dir(path, "creating log directory")?;

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| AutospecError::io("opening log file", path, source))?;
    writeln!(file, "{line}").map_err(|source| AutospecError::io("writing log file", path, source))
}

pub fn reset_run_logs(log_dir: Option<&Path>, slug: &str) -> Result<Option<PathBuf>> {
    let Some(log_dir) = log_dir else {
        return Ok(None);
    };

    fs::create_dir_all(log_dir)
        .map_err(|source| AutospecError::io("creating output directory", log_dir, source))?;

    for entry in fs::read_dir(log_dir)
        .map_err(|source| AutospecError::io("reading log directory", log_dir, source))?
    {
        let entry = entry
            .map_err(|source| AutospecError::io("reading log directory entry", log_dir, source))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{slug}_iter")) && name.ends_with(".md") {
            fs::remove_file(entry.path()).map_err(|source| {
                AutospecError::io("removing iteration log", entry.path(), source)
            })?;
        }
    }

    let run_log = log_dir.join(format!("{slug}.log"));
    if run_log.exists() {
        fs::remove_file(&run_log)
            .map_err(|source| AutospecError::io("resetting run log", &run_log, source))?;
    }
    Ok(Some(run_log))
}

fn ensure_results_file(results_file: &Path) -> Result<()> {
    ensure_parent_dir(results_file, "creating output directory")?;

    let needs_header = match fs::metadata(results_file) {
        Ok(metadata) => metadata.len() == 0,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => true,
        Err(source) => {
            return Err(AutospecError::io(
                "reading results file metadata",
                results_file,
                source,
            ));
        }
    };

    if needs_header {
        fs::write(results_file, "doc\titerations\tstatus\tdelta\ttimestamp\n")
            .map_err(|source| AutospecError::io("writing results header", results_file, source))?;
    }

    Ok(())
}

fn ensure_parent_dir(path: &Path, operation: &'static str) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|source| AutospecError::io(operation, parent, source))
}
