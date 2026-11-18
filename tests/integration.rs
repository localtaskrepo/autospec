use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn copy_fixture(src: &Path, dest: &Path) {
    fs::create_dir_all(dest).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let target = dest.join(entry.file_name());
        if path.is_dir() {
            copy_fixture(&path, &target);
        } else {
            fs::copy(&path, &target).unwrap();
        }
    }
}

fn fake_agent_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_agent.py")
}

#[test]
fn no_op_custom_agent_converges() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-basic"),
        &docs_dir,
    );

    let template = format!(
        "python3 {} --mode no-op --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "docs/product.md",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Converged:     1"));

    let results = fs::read_to_string(temp.path().join(".autospec/results.tsv")).unwrap();
    assert!(results.contains("docs/product.md\t1\tconverged"));
}

#[test]
fn one_pass_custom_agent_near_converges() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-basic"),
        &docs_dir,
    );

    let template = format!(
        "python3 {} --mode one-pass --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "docs/product.md",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
            "--threshold",
            "4",
        ])
        .assert()
        .success();

    let updated = fs::read_to_string(docs_dir.join("product.md")).unwrap();
    assert!(updated.contains("fake-agent: one-pass refinement"));
}

#[test]
fn ripple_mode_tracks_secondary_changes() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-ripple"),
        &docs_dir,
    );

    let template = format!(
        "python3 {} --mode ripple --prompt {{prompt}} --cwd {{cwd}} --log {{log}} --other docs/feature.md",
        fake_agent_path().display()
    );

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "docs/entity.md",
            "--scope",
            "ripple",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("also touched: docs/feature.md"));

    let entity = fs::read_to_string(docs_dir.join("entity.md")).unwrap();
    let feature = fs::read_to_string(docs_dir.join("feature.md")).unwrap();
    assert!(entity.contains("fake-agent: ripple primary"));
    assert!(feature.contains("fake-agent: ripple secondary"));
}

#[test]
fn sweep_mode_tracks_multiple_files() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-sweep"),
        &docs_dir,
    );

    let template = format!(
        "python3 {} --mode sweep --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "--scope",
            "sweep",
            "docs",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 file(s) changed"));

    let one = fs::read_to_string(docs_dir.join("one.md")).unwrap();
    let two = fs::read_to_string(docs_dir.join("two.md")).unwrap();
    assert!(one.contains("fake-agent: sweep one.md"));
    assert!(two.contains("fake-agent: sweep two.md"));
}

#[test]
fn no_artifacts_keeps_repo_clean() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-basic"),
        &docs_dir,
    );

    let template = format!(
        "python3 {} --mode no-op --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );
    let system_temp = tempdir().unwrap();

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .env("TMPDIR", system_temp.path())
        .env("TMP", system_temp.path())
        .env("TEMP", system_temp.path())
        .args([
            "docs/product.md",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
            "--no-artifacts",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Results:       (disabled via --no-artifacts)",
        ));

    assert!(!temp.path().join(".autospec").exists());
    let left: Vec<_> = fs::read_dir(system_temp.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(left.is_empty(), "left in the temp folder: {left:?}");
}

#[test]
fn empty_results_file_is_reinitialized_before_append() {
    let temp = tempdir().unwrap();
    let docs_dir = temp.path().join("docs");
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-basic"),
        &docs_dir,
    );

    let autospec_dir = temp.path().join(".autospec");
    fs::create_dir_all(&autospec_dir).unwrap();
    fs::write(autospec_dir.join("results.tsv"), "").unwrap();

    let template = format!(
        "python3 {} --mode no-op --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );

    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(temp.path())
        .args([
            "docs/product.md",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
        ])
        .assert()
        .success();

    let results = fs::read_to_string(autospec_dir.join("results.tsv")).unwrap();
    assert!(results.starts_with("doc\titerations\tstatus\tdelta\ttimestamp\n"));
    assert!(results.contains("docs/product.md\t1\tconverged"));
}

#[test]
fn help_output_describes_key_flags() {
    Command::cargo_bin("autospec")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Run an AI coding agent in a convergence loop against markdown docs.",
        ))
        .stdout(predicate::str::contains("--scope <SCOPE>"))
        .stdout(predicate::str::contains("strict edits only the target doc"))
        .stdout(predicate::str::contains("--no-artifacts"))
        .stdout(predicate::str::contains(
            "Do not write repo-local .autospec artifacts",
        ))
        .stdout(predicate::str::contains("Environment overrides:"));
}

fn owner_question_run(temp: &Path) -> Command {
    copy_fixture(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/docs-basic"),
        &temp.join("docs"),
    );
    let template = format!(
        "python3 {} --mode owner-question --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );
    let mut command = Command::cargo_bin("autospec").unwrap();
    command.current_dir(temp).args([
        "docs/product.md",
        "--agent",
        "custom",
        "--agent-cmd",
        &template,
        "--no-commit",
        "--allow-dirty",
    ]);
    command
}

#[test]
fn open_owner_decision_ends_needs_owner() {
    let temp = tempdir().unwrap();

    owner_question_run(temp.path())
        .assert()
        .code(3)
        .stdout(predicate::str::contains("Needs owner:   1"))
        .stdout(predicate::str::contains(
            "docs/product.md: How long do we keep data?",
        ))
        .stdout(predicate::str::contains("a) 30 days (recommended)"));

    let results = fs::read_to_string(temp.path().join(".autospec/results.tsv")).unwrap();
    assert!(results.contains("docs/product.md\t2\tneeds-owner"));
    let doc = fs::read_to_string(temp.path().join("docs/product.md")).unwrap();
    assert!(doc.contains("- initial requirement"));
    assert_eq!(doc.matches("[NEEDS OWNER:").count(), 1);
}

#[test]
fn ask_lotar_requires_a_task() {
    let temp = tempdir().unwrap();

    owner_question_run(temp.path())
        .args(["--ask", "lotar"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("--ask lotar requires --ask-task"));
}

#[cfg(unix)]
#[test]
fn ask_lotar_files_questions_and_writes_back_answers() {
    let temp = tempdir().unwrap();
    let path = fake_lotar_path(temp.path());
    let log = temp.path().join("lotar.log");

    // First run: the question is filed once on the task.
    owner_question_run(temp.path())
        .args(["--ask", "lotar", "--ask-task", "DOCS-1"])
        .env("PATH", &path)
        .env("FAKE_LOTAR_LOG", &log)
        .assert()
        .code(3)
        .stdout(predicate::str::contains("Asked on DOCS-1"));
    let calls = fs::read_to_string(&log).unwrap();
    assert_eq!(calls.lines().count(), 1);
    assert!(calls.contains(r#""--question", "docs/product.md: How long do we keep data?""#));
    assert!(calls.contains(r#""--option", "a: 30 days", "--option", "b: 1 year""#));
    assert!(calls.contains(r#""--recommend", "a""#));

    // Second run: the owner answered b, relayed by triage (ASP-10); the answer replaces the marker and is recorded.
    let comments = temp.path().join("comments.json");
    fs::write(
        &comments,
        r#"{"items": [
          {"date": "2026-11-18T00:00:00+00:00", "text": "docs/product.md: How long do we keep data?\n\nOptions:\n- a: 30 days (recommended)\n- b: 1 year",
           "question": {"options": [{"key": "a", "label": "30 days"}, {"key": "b", "label": "1 year"}], "recommended": "a"}},
          {"date": "2026-11-18T01:00:00+00:00", "text": "Answer: b (1 year)\n\n(answered in INBOX-9)", "answer": {"to": "2026-11-18T00:00:00+00:00", "choice": ["b"], "note": "(answered in INBOX-9)"}}
        ]}"#,
    )
    .unwrap();
    run_with_lotar_answers(temp.path(), &path, &log, &comments)
        .success()
        .stdout(predicate::str::contains("Converged:     1"));

    let doc = fs::read_to_string(temp.path().join("docs/product.md")).unwrap();
    assert!(!doc.contains("[NEEDS OWNER:"));
    assert!(doc.contains("- retention: 1 year\n"));
    assert!(doc.contains("## Clarifications\n\n### Session "));
    assert!(doc.contains("- Q: How long do we keep data? → A: 1 year\n"));
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
}

#[cfg(unix)]
#[test]
fn ask_lotar_writes_back_a_plain_relayed_answer() {
    // Triage relays the owner's answer as a plain comment when `--answer` fails (ASP-7).
    let temp = tempdir().unwrap();
    let path = fake_lotar_path(temp.path());
    let log = temp.path().join("lotar.log");
    owner_question_run(temp.path())
        .args(["--ask", "lotar", "--ask-task", "DOCS-1"])
        .env("PATH", &path)
        .env("FAKE_LOTAR_LOG", &log)
        .assert()
        .code(3);

    let comments = temp.path().join("comments.json");
    fs::write(
        &comments,
        r#"{"items": [
          {"date": "2026-11-18T00:00:00+00:00", "text": "docs/product.md: How long do we keep data?\n\nOptions:\n- a: 30 days (recommended)\n- b: 1 year",
           "question": {"options": [{"key": "a", "label": "30 days"}, {"key": "b", "label": "1 year"}], "recommended": "a"}},
          {"date": "2026-11-18T00:30:00+00:00", "author": "claude", "text": "Filed the question."},
          {"date": "2026-11-18T01:00:00+00:00", "author": "mallox", "text": "Answer: 2 years (answered in INBOX-9)"}
        ]}"#,
    )
    .unwrap();
    run_with_lotar_answers(temp.path(), &path, &log, &comments)
        .success()
        .stdout(predicate::str::contains("Converged:     1"));

    let doc = fs::read_to_string(temp.path().join("docs/product.md")).unwrap();
    assert!(!doc.contains("[NEEDS OWNER:"));
    assert!(doc.contains("- retention: 2 years\n"));
    assert!(doc.contains("- Q: How long do we keep data? → A: 2 years\n"));
}

/// A `bin` folder in `root` with the fake lotar, and a PATH that finds it first.
#[cfg(unix)]
fn fake_lotar_path(root: &Path) -> String {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/support/fake_lotar.py"),
        bin.join("lotar"),
    )
    .unwrap();
    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap())
}

/// The run after the owner answered: lotar lists `comments`, the agent changes nothing.
#[cfg(unix)]
fn run_with_lotar_answers(
    root: &Path,
    path: &str,
    log: &Path,
    comments: &Path,
) -> assert_cmd::assert::Assert {
    let template = format!(
        "python3 {} --mode no-op --prompt {{prompt}} --cwd {{cwd}} --log {{log}}",
        fake_agent_path().display()
    );
    Command::cargo_bin("autospec")
        .unwrap()
        .current_dir(root)
        .args([
            "docs/product.md",
            "--agent",
            "custom",
            "--agent-cmd",
            &template,
            "--no-commit",
            "--allow-dirty",
            "--ask",
            "lotar",
            "--ask-task",
            "DOCS-1",
        ])
        .env("PATH", path)
        .env("FAKE_LOTAR_LOG", log)
        .env("FAKE_LOTAR_COMMENTS", comments)
        .assert()
}
