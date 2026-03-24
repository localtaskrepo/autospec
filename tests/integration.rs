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
            "--no-artifacts",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Results:       (disabled via --no-artifacts)",
        ));

    assert!(!temp.path().join(".autospec").exists());
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
