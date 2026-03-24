use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::agent::{AgentRunRequest, AgentRunResult, ResolvedAgent, run_agent};
use crate::config::RuntimeConfig;
use crate::diff::scope_diff;
use crate::docs::ScopeDiscovery;
use crate::error::Result;
use crate::git::stage_and_commit_changes;
use crate::output::{OutputPaths, append_log, append_result, reset_run_logs};
use crate::prompt::{DEFAULT_PROMPT, PromptContext, build_prompt};
use crate::state::snapshot_scope;

use super::{RunPlan, print_change_summary};

type AgentExecutor = fn(&ResolvedAgent, &AgentRunRequest) -> Result<AgentRunResult>;

struct IterationOutcome<'a> {
    status: &'a str,
    delta: &'a str,
    converged: bool,
}

pub(super) struct ConvergenceRunner<'a> {
    config: &'a RuntimeConfig,
    agent: &'a ResolvedAgent,
    discovery: &'a ScopeDiscovery,
    outputs: &'a OutputPaths,
    plan: &'a RunPlan,
    agent_executor: AgentExecutor,
    run_log: Option<PathBuf>,
    low_delta_streak: u32,
    last_delta: String,
    last_changed_files: usize,
    seen_states: BTreeSet<String>,
}

impl<'a> ConvergenceRunner<'a> {
    pub(super) fn new(
        config: &'a RuntimeConfig,
        agent: &'a ResolvedAgent,
        discovery: &'a ScopeDiscovery,
        outputs: &'a OutputPaths,
        plan: &'a RunPlan,
    ) -> Result<Self> {
        Self::with_executor(config, agent, discovery, outputs, plan, run_agent)
    }

    fn with_executor(
        config: &'a RuntimeConfig,
        agent: &'a ResolvedAgent,
        discovery: &'a ScopeDiscovery,
        outputs: &'a OutputPaths,
        plan: &'a RunPlan,
        agent_executor: AgentExecutor,
    ) -> Result<Self> {
        let run_log = reset_run_logs(outputs.log_dir(), &plan.slug)?;
        let initial_snapshot = snapshot_scope(&config.repo_root, &plan.tracked_files)?;

        Ok(Self {
            config,
            agent,
            discovery,
            outputs,
            plan,
            agent_executor,
            run_log,
            low_delta_streak: 0,
            last_delta: String::new(),
            last_changed_files: 0,
            seen_states: BTreeSet::from([initial_snapshot.hash]),
        })
    }

    pub(super) fn run(mut self) -> Result<bool> {
        for iteration in 1..=self.config.max_iters {
            if let Some(done) = self.run_iteration(iteration)? {
                return Ok(done);
            }
        }

        println!(
            "  ✗ Did not converge after {} iterations",
            self.config.max_iters
        );
        append_result(
            self.outputs.results_file(),
            &self.plan.results_key,
            self.config.max_iters,
            "not-converged",
            &self.last_delta,
        )?;
        Ok(false)
    }

    fn run_iteration(&mut self, iteration: u32) -> Result<Option<bool>> {
        println!("── iteration {iteration}/{}", self.config.max_iters);

        let before = snapshot_scope(&self.config.repo_root, &self.plan.tracked_files)?;
        let agent_result = (self.agent_executor)(self.agent, &self.agent_request(iteration))?;

        match agent_result {
            AgentRunResult::TimedOut => {
                return self
                    .finish_iteration(
                        iteration,
                        IterationOutcome {
                            status: "agent-timeout",
                            delta: "",
                            converged: false,
                        },
                        format!("[{iteration}] agent-timeout"),
                    )
                    .map(Some);
            }
            AgentRunResult::Failed => {
                return self
                    .finish_iteration(
                        iteration,
                        IterationOutcome {
                            status: "agent-failed",
                            delta: "",
                            converged: false,
                        },
                        format!("[{iteration}] agent-failed"),
                    )
                    .map(Some);
            }
            AgentRunResult::Completed => {}
        }

        let after = snapshot_scope(&self.config.repo_root, &self.plan.tracked_files)?;
        if before == after {
            return self.finish_no_diff(iteration).map(Some);
        }

        let Some(delta) = scope_diff(&before, &after) else {
            return self.finish_no_diff(iteration).map(Some);
        };

        self.last_delta = delta.display.clone();
        self.last_changed_files = delta.files.len();

        if self.seen_states.contains(&after.hash) {
            println!("  ⚠ Oscillation detected ({})", delta.display);
            self.record_iteration_status(
                iteration,
                "oscillating",
                &delta.display,
                format!("[{iteration}] oscillating ({})", delta.display),
            )?;
            return Ok(Some(false));
        }
        self.seen_states.insert(after.hash.clone());

        if let Some(done) = self.apply_iteration_changes(iteration, &before, &after, &delta)? {
            return Ok(Some(done));
        }
        self.record_iteration_delta(iteration, &delta)?;
        self.update_low_delta_streak(&delta);

        if iteration >= 2 && self.low_delta_streak >= self.config.stable_iters {
            println!(
                "  ✓ Near-converged ({}) after {iteration} iteration(s)",
                delta.display
            );
            return self
                .finish_iteration(
                    iteration,
                    IterationOutcome {
                        status: "converged",
                        delta: &delta.display,
                        converged: true,
                    },
                    format!("[{iteration}] near-converged"),
                )
                .map(Some);
        }

        Ok(None)
    }

    fn agent_request(&self, iteration: u32) -> AgentRunRequest {
        build_agent_request(
            self.config,
            self.discovery,
            self.outputs,
            self.plan,
            iteration,
            self.last_delta.clone(),
            self.last_changed_files,
        )
    }

    fn apply_iteration_changes(
        &mut self,
        iteration: u32,
        before: &crate::state::ScopeSnapshot,
        after: &crate::state::ScopeSnapshot,
        delta: &crate::diff::ScopeDelta,
    ) -> Result<Option<bool>> {
        if !self.config.no_commit {
            let changed_files = delta
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>();
            if let Err(error) = stage_and_commit_changes(
                &self.config.repo_root,
                &changed_files,
                &format!(
                    "autospec: {} - iteration {iteration}",
                    self.plan.results_key
                ),
            ) {
                println!("  ✗ Commit failed: {error}");
                self.record_iteration_status(
                    iteration,
                    "commit-failed",
                    &delta.display,
                    format!("[{iteration}] commit-failed: {error}"),
                )?;
                return Ok(Some(false));
            }
            print_change_summary(self.plan, before, after, delta, "committing");
        } else {
            print_change_summary(
                self.plan,
                before,
                after,
                delta,
                "keeping working tree changes",
            );
        }

        Ok(None)
    }

    fn record_iteration_delta(
        &self,
        iteration: u32,
        delta: &crate::diff::ScopeDelta,
    ) -> Result<()> {
        append_log(
            self.run_log.as_deref(),
            &format!(
                "[{iteration}] changed ({}) files={}",
                delta.display,
                delta
                    .files
                    .iter()
                    .map(|file| file.path.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        )
    }

    fn update_low_delta_streak(&mut self, delta: &crate::diff::ScopeDelta) {
        if delta.total_changed() <= self.config.threshold {
            self.low_delta_streak += 1;
        } else {
            self.low_delta_streak = 0;
        }
    }

    fn finish_no_diff(&self, iteration: u32) -> Result<bool> {
        println!("  ✓ No changes - converged after {iteration} iteration(s)");
        self.finish_iteration(
            iteration,
            IterationOutcome {
                status: "converged",
                delta: "",
                converged: true,
            },
            format!("[{iteration}] no-diff -> converged"),
        )
    }

    fn finish_iteration(
        &self,
        iteration: u32,
        outcome: IterationOutcome<'_>,
        log_line: impl AsRef<str>,
    ) -> Result<bool> {
        self.record_iteration_status(iteration, outcome.status, outcome.delta, log_line)?;
        Ok(outcome.converged)
    }

    fn record_iteration_status(
        &self,
        iteration: u32,
        status: &str,
        delta: &str,
        log_line: impl AsRef<str>,
    ) -> Result<()> {
        append_log(self.run_log.as_deref(), log_line.as_ref())?;
        append_result(
            self.outputs.results_file(),
            &self.plan.results_key,
            iteration,
            status,
            delta,
        )
    }
}

pub(super) fn build_agent_request(
    config: &RuntimeConfig,
    discovery: &ScopeDiscovery,
    outputs: &OutputPaths,
    plan: &RunPlan,
    iteration: u32,
    last_delta: String,
    last_changed_files: usize,
) -> AgentRunRequest {
    AgentRunRequest {
        prompt: build_prompt(
            DEFAULT_PROMPT,
            &PromptContext {
                doc: plan.prompt_doc.clone(),
                scope: config.scope,
                scope_dir: discovery.scope_dir.clone(),
                goal: config.goal.clone(),
                iteration,
                max_iters: config.max_iters,
                last_delta,
                scope_file_count: plan.tracked_files.len(),
                last_changed_files,
            },
        ),
        log_path: outputs.iteration_log_path(&plan.slug, iteration),
        model: config.model.clone(),
        effort: config.effort.clone(),
        timeout: config.agent_timeout,
        cwd: config.repo_root.clone(),
        dry_run: config.dry_run,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::tempdir;

    use crate::config::{AgentRequest, ScopeMode};
    use crate::docs::discover_scope;
    use crate::output::output_paths;

    fn no_op_agent(_: &ResolvedAgent, _: &AgentRunRequest) -> Result<AgentRunResult> {
        Ok(AgentRunResult::Completed)
    }

    fn oscillating_agent(_: &ResolvedAgent, request: &AgentRunRequest) -> Result<AgentRunResult> {
        let path = request.cwd.join("docs/product.md");
        let marker = "- oscillate";
        let text = fs::read_to_string(&path).unwrap_or_default();
        let next = if text.contains(marker) {
            text.replace(&(marker.to_owned() + "\n"), "")
                .replace(marker, "")
        } else if text.ends_with('\n') {
            format!("{text}{marker}\n")
        } else {
            format!("{text}\n{marker}\n")
        };
        fs::write(path, next).unwrap();
        Ok(AgentRunResult::Completed)
    }

    fn two_small_deltas_agent(
        _: &ResolvedAgent,
        request: &AgentRunRequest,
    ) -> Result<AgentRunResult> {
        let path = request.cwd.join("docs/product.md");
        let text = fs::read_to_string(&path).unwrap_or_default();
        let count = text
            .lines()
            .filter(|line| line.contains("near-step-"))
            .count();
        if count < 2 {
            let next_line = format!("- near-step-{}", count + 1);
            let next = if text.ends_with('\n') {
                format!("{text}{next_line}\n")
            } else if text.is_empty() {
                format!("{next_line}\n")
            } else {
                format!("{text}\n{next_line}\n")
            };
            fs::write(path, next).unwrap();
        }
        Ok(AgentRunResult::Completed)
    }

    fn setup_runner(
        max_iters: u32,
        threshold: usize,
        stable_iters: u32,
        executor: AgentExecutor,
    ) -> (
        tempfile::TempDir,
        RuntimeConfig,
        ScopeDiscovery,
        OutputPaths,
        RunPlan,
        ResolvedAgent,
    ) {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("docs")).unwrap();
        fs::write(
            temp.path().join("docs/product.md"),
            "# Product\n\n- initial requirement\n",
        )
        .unwrap();

        let config = RuntimeConfig {
            repo_root: temp.path().to_path_buf(),
            target: temp.path().join("docs/product.md"),
            scope: ScopeMode::Strict,
            goal: String::new(),
            max_iters,
            threshold,
            stable_iters,
            agent_request: AgentRequest::Custom,
            agent_cmd_template: Some("test".to_owned()),
            model: "gpt-5.4".to_owned(),
            effort: String::new(),
            agent_timeout: None,
            skip_readmes: false,
            allow_dirty: true,
            no_commit: true,
            no_branch: true,
            dry_run: false,
            no_artifacts: false,
            max_scope_files: None,
        };

        let discovery = discover_scope(&config).unwrap();
        let outputs = output_paths(&config.repo_root, config.no_artifacts);
        let plan = RunPlan {
            label: "docs/product.md".to_owned(),
            slug: "docs__product".to_owned(),
            prompt_doc: "docs/product.md".to_owned(),
            results_key: "docs/product.md".to_owned(),
            tracked_files: vec!["docs/product.md".to_owned()],
            focus_doc: Some("docs/product.md".to_owned()),
        };
        let agent = ResolvedAgent::Custom {
            template: "test".to_owned(),
        };

        let runner = ConvergenceRunner::with_executor(
            &config, &agent, &discovery, &outputs, &plan, executor,
        )
        .unwrap();

        assert_eq!(runner.seen_states.len(), 1);

        drop(runner);
        (temp, config, discovery, outputs, plan, agent)
    }

    #[test]
    fn runner_converges_on_no_diff() {
        let (_temp, config, discovery, outputs, plan, agent) = setup_runner(3, 10, 2, no_op_agent);

        let result = ConvergenceRunner::with_executor(
            &config,
            &agent,
            &discovery,
            &outputs,
            &plan,
            no_op_agent,
        )
        .unwrap()
        .run()
        .unwrap();

        assert!(result);
        let results = fs::read_to_string(outputs.results_file().unwrap()).unwrap();
        assert!(results.contains("docs/product.md\t1\tconverged"));
    }

    #[test]
    fn runner_detects_oscillation() {
        let (_temp, config, discovery, outputs, plan, agent) =
            setup_runner(4, 10, 2, oscillating_agent);

        let result = ConvergenceRunner::with_executor(
            &config,
            &agent,
            &discovery,
            &outputs,
            &plan,
            oscillating_agent,
        )
        .unwrap()
        .run()
        .unwrap();

        assert!(!result);
        let results = fs::read_to_string(outputs.results_file().unwrap()).unwrap();
        assert!(results.contains("docs/product.md\t2\toscillating"));
    }

    #[test]
    fn runner_near_converges_after_two_small_deltas() {
        let (_temp, config, discovery, outputs, plan, agent) =
            setup_runner(5, 10, 2, two_small_deltas_agent);

        let result = ConvergenceRunner::with_executor(
            &config,
            &agent,
            &discovery,
            &outputs,
            &plan,
            two_small_deltas_agent,
        )
        .unwrap()
        .run()
        .unwrap();

        assert!(result);
        let results = fs::read_to_string(outputs.results_file().unwrap()).unwrap();
        assert!(results.contains("docs/product.md\t2\tconverged\t+1/-0"));
    }
}
