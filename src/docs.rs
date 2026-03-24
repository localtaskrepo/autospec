use std::fs;
use std::path::Path;

use crate::config::{RuntimeConfig, ScopeMode};
use crate::error::{AutospecError, Result};

#[derive(Debug, Clone)]
pub struct ScopeDiscovery {
    pub target_docs: Vec<String>,
    pub scope_files: Vec<String>,
    pub scope_dir: String,
    pub warnings: Vec<String>,
}

pub fn discover_scope(config: &RuntimeConfig) -> Result<ScopeDiscovery> {
    let target_docs = collect_docs(&config.target, &config.repo_root, config.skip_readmes, true)?;
    if target_docs.is_empty() {
        return Err(AutospecError::EmptyScope(format!(
            "{} contains no markdown documents",
            repo_relative(&config.repo_root, &config.target)?
        )));
    }

    let scope_dir_path = if config.target.is_file() {
        config
            .target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| config.repo_root.clone())
    } else {
        config.target.clone()
    };
    let scope_dir = repo_relative(&config.repo_root, &scope_dir_path)?;
    let scope_files = collect_docs(
        &scope_dir_path,
        &config.repo_root,
        config.skip_readmes,
        false,
    )?;

    if matches!(config.scope, ScopeMode::Ripple | ScopeMode::Sweep) && scope_files.is_empty() {
        return Err(AutospecError::EmptyScope(format!(
            "{} contains no markdown documents",
            scope_dir
        )));
    }

    if matches!(config.scope, ScopeMode::Ripple | ScopeMode::Sweep)
        && config
            .max_scope_files
            .is_some_and(|cap| scope_files.len() > cap)
    {
        return Err(AutospecError::InvalidConfig(format!(
            "scope contains {} files, exceeding --max-scope-files={}",
            scope_files.len(),
            config.max_scope_files.unwrap_or_default()
        )));
    }

    let warning_files = if config.scope == ScopeMode::Strict {
        &target_docs
    } else {
        &scope_files
    };

    let warnings = sparse_seed_warnings(warning_files, &config.repo_root)?;

    Ok(ScopeDiscovery {
        target_docs,
        scope_files,
        scope_dir,
        warnings,
    })
}

pub fn repo_relative(repo_root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(repo_root)
        .map_err(|_| AutospecError::TargetOutsideRepo(path.to_path_buf()))?;

    let display = relative.to_string_lossy().replace('\\', "/");
    if display.is_empty() {
        Ok(".".to_owned())
    } else {
        Ok(display)
    }
}

pub fn read_text_allow_missing(path: &Path) -> Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(source) => Err(AutospecError::io("reading file", path, source)),
    }
}

pub fn count_nonempty_lines(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

fn collect_docs(
    target: &Path,
    repo_root: &Path,
    skip_readmes: bool,
    include_explicit_file: bool,
) -> Result<Vec<String>> {
    if target.is_file() {
        let rel = repo_relative(repo_root, target)?;
        if skip_readmes
            && !include_explicit_file
            && Path::new(&rel)
                .file_name()
                .is_some_and(|name| name == "README.md")
        {
            return Ok(Vec::new());
        }
        return Ok(vec![rel]);
    }

    if !target.is_dir() {
        return Err(AutospecError::MissingTarget(target.to_path_buf()));
    }

    let mut docs = Vec::new();
    collect_docs_recursive(target, repo_root, skip_readmes, &mut docs)?;

    docs.sort();
    Ok(docs)
}

fn collect_docs_recursive(
    directory: &Path,
    repo_root: &Path,
    skip_readmes: bool,
    docs: &mut Vec<String>,
) -> Result<()> {
    for entry in fs::read_dir(directory)
        .map_err(|source| AutospecError::io("reading docs directory", directory, source))?
    {
        let entry = entry.map_err(|source| {
            AutospecError::io("reading docs directory entry", directory, source)
        })?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|source| AutospecError::io("reading docs file type", &path, source))?;

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            collect_docs_recursive(&path, repo_root, skip_readmes, docs)?;
            continue;
        }

        if !file_type.is_file() || path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }

        let rel = repo_relative(repo_root, &path)?;
        if skip_readmes
            && Path::new(&rel)
                .file_name()
                .is_some_and(|name| name == "README.md")
        {
            continue;
        }
        docs.push(rel);
    }

    Ok(())
}

fn sparse_seed_warnings(files: &[String], repo_root: &Path) -> Result<Vec<String>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }

    let counts = files
        .iter()
        .map(|file| {
            read_text_allow_missing(&repo_root.join(file)).map(|text| count_nonempty_lines(&text))
        })
        .collect::<Result<Vec<_>>>()?;
    let total_nonempty: usize = counts.iter().sum();

    let warning = if files.len() == 1 && total_nonempty <= 2 {
        Some("WARNING: seed docs are almost empty. autospec will likely converge to a conservative requirements-unspecified baseline rather than inventing a project spec.".to_owned())
    } else if files.len() == 1 && total_nonempty <= 12 {
        Some("WARNING: only one short seed doc was found. autospec can bootstrap from this, but the result will usually stay generic unless the doc names concrete entities, states, and constraints.".to_owned())
    } else if files.len() <= 3 && total_nonempty <= 30 {
        Some("WARNING: the in-scope docs are very sparse. autospec works best when there is at least one reasonably concrete seed doc to anchor terminology and scope.".to_owned())
    } else {
        None
    };

    Ok(warning.into_iter().collect())
}
