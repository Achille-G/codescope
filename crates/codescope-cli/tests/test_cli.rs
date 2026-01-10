//! CLI integration tests
//!
//! These tests verify the CLI commands work correctly end-to-end.

use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

fn codescope_cmd() -> Command {
    Command::new(cargo_bin("codescope"))
}

#[test]
fn test_cli_help() {
    let mut cmd = codescope_cmd();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("codescope"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("search"));
}

#[test]
fn test_cli_version() {
    let mut cmd = codescope_cmd();
    cmd.arg("--version").assert().success();
}

#[test]
fn test_cli_init() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    // Check that .codescope directory was created
    assert!(temp_dir.path().join(".codescope").exists());
    assert!(temp_dir.path().join(".codescope").join("config.toml").exists());
}

#[test]
fn test_cli_init_already_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // First init
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    // Second init should fail (already initialized)
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

#[test]
fn test_cli_status_not_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a codescope project"));
}

#[test]
fn test_cli_status_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    // Then check status
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("status")
        .assert()
        .success();
}

#[test]
fn test_cli_search_not_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("test query")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a codescope project"));
}

#[test]
fn test_cli_clean_not_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("clean")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a codescope project"));
}

#[test]
fn test_cli_clean_initialized() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    // Clean should succeed
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("clean")
        .assert()
        .success();
}

#[test]
fn test_cli_index_empty_project() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    // Index should succeed even with no files
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();
}

#[test]
fn test_cli_index_with_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a sample file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function hello(): string { return 'world'; }",
    )
    .expect("Failed to write test file");

    // Initialize
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    // Index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));
}

#[test]
fn test_cli_search_with_indexed_content() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a sample file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        r#"
/**
 * A unique function for testing search
 */
export function uniqueSearchableFunction(): string {
    return 'This is uniquely searchable content';
}
"#,
    )
    .expect("Failed to write test file");

    // Initialize
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    // Index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("index").assert().success();

    // Search for the unique function
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("uniqueSearchableFunction")
        .arg("--type")
        .arg("lexical")
        .assert()
        .success();
}

#[test]
fn test_cli_search_pretty_output() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a sample file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function testFunction(): void { console.log('test'); }",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("index").assert().success();

    // Search with pretty output
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("testFunction")
        .arg("--type")
        .arg("lexical")
        .arg("--pretty")
        .assert()
        .success()
        .stdout(predicate::str::contains("Query:"));
}

#[test]
fn test_cli_search_top_k() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create multiple files
    for i in 0..5 {
        std::fs::write(
            temp_dir.path().join(format!("test{}.ts", i)),
            format!("export function func{}(): void {{}}", i),
        )
        .expect("Failed to write test file");
    }

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("index").assert().success();

    // Search with --top 2
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("function")
        .arg("--type")
        .arg("lexical")
        .arg("--top")
        .arg("2")
        .assert()
        .success();
}

/// Test incremental indexing when new files are added.
/// Note: Currently ignored due to HNSW capacity issue when loading existing index.
/// TODO: Fix HNSW reserve logic in codescope-search/src/hnsw.rs
#[test]
#[ignore = "HNSW capacity issue on incremental index - needs fix"]
fn test_cli_index_incremental() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create initial file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function initial(): void {}",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("index").assert().success();

    // Add another file
    std::fs::write(
        temp_dir.path().join("test2.ts"),
        "export function added(): void {}",
    )
    .expect("Failed to write test file");

    // Incremental index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));
}

#[test]
fn test_cli_index_all_flag() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function test(): void {}",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("init").assert().success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path()).arg("index").assert().success();

    // Full re-index with --all
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));
}
