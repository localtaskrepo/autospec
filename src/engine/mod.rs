use crate::agent::{ResolvedAgent, list_builtin_availability, resolve_agent, run_agent};
use crate::config::{RuntimeConfig, ScopeMode};
use crate::diff::diff_file;
use crate::docs::{ScopeDiscovery, discover_scope, repo_relative};
use crate::error::{AutospecError, Result};
use crate::git::{create_branch, dirty_docs};
use crate::output::{OutputPaths, ensure_output_paths, output_paths};

mod runner;

use runner::ConvergenceRunner;

#[derive(Debug, Clone)]
struct RunPlan {
    label: String,
    slug: String,
    prompt_doc: String,
    results_key: String,
    tracked_files: Vec<String>,
    focus_doc: Option<String>,
}

pub fn run(config: RuntimeConfig) -> Result<()> {
    let discovery = discover_scope(&config)?;
    let agent = resolve_agent(&config)?;
    let outputs = output_paths(&config.repo_root);

    print_header(&config, &discovery, &agent);
    for warning in &discovery.warnings {
        eprintln!("{warning}");
    }

    if !config.dry_run {
        ensure_output_paths(&outputs)?;
    }

    if !config.dry_run && !config.allow_dirty {
        match config.scope {
            ScopeMode::Strict => {}
            ScopeMode::Ripple | ScopeMode::Sweep => {
                let dirty = dirty_docs(&config.repo_root, &discovery.scope_files)?;
                if !dirty.is_empty() {
                    return Err(AutospecError::DirtyDocs(dirty.join(", ")));
                }
            }
        }
    }

    if !config.dry_run && !config.no_commit && !config.no_branch {
        let branch = format!("autospec/{}", chrono::Local::now().format("%Y%m%d-%H%M"));
        create_branch(&config.repo_root, &branch)?;
        println!("branch:     {branch}");
    }

    match config.scope {
        ScopeMode::Sweep => run_sweep(&config, &agent, &discovery, &outputs),
        ScopeMode::Strict | ScopeMode::Ripple => run_docs(&config, &agent, &discovery, &outputs),
    }
}
fn run_sweep(
    config: &RuntimeConfig,
    agent: &ResolvedAgent,
    discovery: &ScopeDiscovery,
    outputs: &OutputPaths,
) -> Result<()> {
    let plan = RunPlan {
        label: format!(
            "{}/ ({} files)",
            discovery.scope_dir,
            discovery.scope_files.len()
        ),
        slug: format!("{}__sweep", slugify(&discovery.scope_dir)),
        prompt_doc: discovery.scope_dir.clone(),
        results_key: format!("sweep:{}", discovery.scope_dir),
        tracked_files: discovery.scope_files.clone(),
        focus_doc: None,
    };

    let converged = convergence_loop(config, agent, discovery, outputs, &plan)?;
    println!("\n{}", "═".repeat(51));
    println!("  SUMMARY");
    println!("{}", "═".repeat(51));
    println!(
        "  Scope:         {}/ ({} files)",
        discovery.scope_dir,
        discovery.scope_files.len()
    );
    println!(
        "  Result:        {}",
        if converged {
            "converged"
        } else {
            "not converged"
        }
    );
    println!("  Results:       {}", results_path_display(config, outputs));
    println!("  Logs:          {}/", outputs.log_dir.display());
    println!("{}", "═".repeat(51));
    Ok(())
}

fn run_docs(
    config: &RuntimeConfig,
    agent: &ResolvedAgent,
    discovery: &ScopeDiscovery,
    outputs: &OutputPaths,
) -> Result<()> {
    let total = discovery.target_docs.len();
    let mut converged = 0u32;
    let mut failed = 0u32;

    for (index, doc) in discovery.target_docs.iter().enumerate() {
        if !config.dry_run && !config.allow_dirty && config.scope == ScopeMode::Strict {
            let dirty = dirty_docs(&config.repo_root, std::slice::from_ref(doc))?;
            if !dirty.is_empty() {
                return Err(AutospecError::DirtyDocs(dirty.join(", ")));
            }
        }

        println!("\n[{}/{}] {}", index + 1, total, doc);
        let tracked = if config.scope == ScopeMode::Ripple {
            discovery.scope_files.clone()
        } else {
            vec![doc.clone()]
        };
        let plan = RunPlan {
            label: if config.scope == ScopeMode::Ripple {
                format!("{} (ripple -> {} files)", doc, discovery.scope_files.len())
            } else {
                doc.clone()
            },
            slug: slugify(doc),
            prompt_doc: doc.clone(),
            results_key: doc.clone(),
            tracked_files: tracked,
            focus_doc: Some(doc.clone()),
        };

        if convergence_loop(config, agent, discovery, outputs, &plan)? {
            converged += 1;
        } else {
            failed += 1;
        }
    }

    println!("\n{}", "═".repeat(51));
    println!("  SUMMARY");
    println!("{}", "═".repeat(51));
    println!("  Total docs:    {total}");
    println!("  Converged:     {converged}");
    println!("  Not converged: {failed}");
    println!("  Results:       {}", results_path_display(config, outputs));
    println!("  Logs:          {}/", outputs.log_dir.display());
    println!("{}", "═".repeat(51));
    Ok(())
}

fn convergence_loop(
    config: &RuntimeConfig,
    agent: &ResolvedAgent,
    discovery: &ScopeDiscovery,
    outputs: &OutputPaths,
    plan: &RunPlan,
) -> Result<bool> {
    let is_sweep = config.scope == ScopeMode::Sweep;

    println!("\n{}", "═".repeat(51));
    println!(
        "  {}: {}",
        if is_sweep { "SWEEP" } else { "DOC" },
        plan.label
    );
    println!("{}", "═".repeat(51));

    if config.dry_run {
        println!("── iteration 1/1");
        run_agent(
            agent,
            &runner::build_agent_request(config, discovery, outputs, plan, 1, String::new(), 0),
        )?;
        println!("  ✓ Dry run complete");
        return Ok(true);
    }

    ConvergenceRunner::new(config, agent, discovery, outputs, plan)?.run()
}

fn results_path_display(config: &RuntimeConfig, outputs: &OutputPaths) -> String {
    if config.dry_run {
        "(not written in dry-run)".to_owned()
    } else {
        outputs.results_file.display().to_string()
    }
}

fn print_header(config: &RuntimeConfig, discovery: &ScopeDiscovery, agent: &ResolvedAgent) {
    println!("autospec - doc convergence loop");
    println!("agent:      {}", agent.display_name());
    println!("model:      {}", config.model);
    if !config.effort.is_empty() {
        println!("effort:     {}", config.effort);
    }
    println!(
        "timeout:    {}",
        config
            .agent_timeout
            .map(|timeout| format!("{}s", timeout.as_secs()))
            .unwrap_or_else(|| "disabled".to_owned())
    );
    println!("scope:      {}", config.scope);
    if let Some(cap) = config.max_scope_files {
        println!("scope cap:  {cap} files");
    }
    if !config.goal.is_empty() {
        println!("goal:       {}", config.goal);
    }
    println!("max iters:  {}", config.max_iters);
    println!("threshold:  {}", config.threshold);
    println!("stable iters: {}", config.stable_iters);
    println!(
        "target:     {}",
        repo_relative(&config.repo_root, &config.target)
            .unwrap_or_else(|_| config.target.display().to_string())
    );
    println!("docs found: {}", discovery.target_docs.len());
    if config.scope != ScopeMode::Strict {
        println!(
            "scope dir:  {}/ ({} files in scope)",
            discovery.scope_dir,
            discovery.scope_files.len()
        );
    }
    println!("commit mode: {}", !config.no_commit);
    println!("dry run:    {}", config.dry_run);
    let availability = list_builtin_availability()
        .into_iter()
        .map(|entry| {
            format!(
                "{}={}",
                entry.kind,
                if entry.executable.is_some() {
                    "yes"
                } else {
                    "no"
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    println!("agents:     {availability}");
    println!();
}

fn print_change_summary(
    plan: &RunPlan,
    before: &crate::state::ScopeSnapshot,
    after: &crate::state::ScopeSnapshot,
    delta: &crate::diff::ScopeDelta,
    action: &str,
) {
    if let Some(focus) = plan.focus_doc.as_ref() {
        if delta.files.len() > 1 {
            let others = delta
                .files
                .iter()
                .map(|file| file.path.as_str())
                .filter(|path| *path != focus)
                .collect::<Vec<_>>();
            if others.is_empty() {
                println!("  -> changes detected ({}), {action}", delta.display);
            } else {
                println!(
                    "  -> changes ({}), {action} - also touched: {}",
                    delta.display,
                    others.join(", ")
                );
            }
        } else {
            println!("  -> changes detected ({}), {action}", delta.display);
        }
    } else {
        println!(
            "  -> {} file(s) changed ({}), {action}",
            delta.files.len(),
            delta.display
        );
        for file in &delta.files {
            let file_before = before
                .files
                .get(&file.path)
                .map(String::as_str)
                .unwrap_or("");
            let file_after = after
                .files
                .get(&file.path)
                .map(String::as_str)
                .unwrap_or("");
            let file_delta = diff_file(file_before, file_after)
                .map(|(_, _, display)| display)
                .unwrap_or_default();
            println!("    {} ({})", file.path, file_delta);
        }
    }
}

fn slugify(value: &str) -> String {
    let slug = value.replace(['/', ':'], "__");
    let slug = slug.trim_end_matches(".md");
    if slug.is_empty() || slug == "." {
        "root".to_owned()
    } else {
        slug.to_owned()
    }
}
