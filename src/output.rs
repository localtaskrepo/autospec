use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{AutospecError, Result};

#[derive(Debug, Clone)]
pub struct OutputPaths {
    pub log_dir: PathBuf,
    pub results_file: PathBuf,
}

pub fn output_paths(repo_root: &Path) -> OutputPaths {
    let base_dir = repo_root.join(".autospec");
    let log_dir = base_dir.join("logs");
    let results_file = base_dir.join("results.tsv");
    OutputPaths {
        log_dir,
        results_file,
    }
}

pub fn ensure_output_paths(paths: &OutputPaths) -> Result<()> {
    fs::create_dir_all(&paths.log_dir)
        .map_err(|source| AutospecError::io("creating output directory", &paths.log_dir, source))?;

    if !paths.results_file.exists() {
        fs::write(
            &paths.results_file,
            "doc\titerations\tstatus\tdelta\ttimestamp\n",
        )
        .map_err(|source| {
            AutospecError::io("writing results header", &paths.results_file, source)
        })?;
    }

    Ok(())
}

pub fn append_result(
    results_file: &Path,
    doc: &str,
    iterations: u32,
    status: &str,
    delta: &str,
) -> Result<()> {
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

pub fn append_log(path: &Path, line: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| AutospecError::io("opening log file", path, source))?;
    writeln!(file, "{line}").map_err(|source| AutospecError::io("writing log file", path, source))
}

pub fn reset_run_logs(log_dir: &Path, slug: &str) -> Result<PathBuf> {
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
    fs::write(&run_log, "")
        .map_err(|source| AutospecError::io("resetting run log", &run_log, source))?;
    Ok(run_log)
}
