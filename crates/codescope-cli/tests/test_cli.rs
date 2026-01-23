//! CLI integration tests
//!
//! These tests verify the CLI commands work correctly end-to-end.

use assert_cmd::assert::OutputAssertExt;
#[allow(deprecated)]
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tempfile::TempDir;

#[allow(deprecated)]
fn codescope_cmd() -> Command {
    Command::new(cargo_bin("codescope"))
}

fn cli_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

macro_rules! cli_serial {
    () => {
        let _guard = cli_test_guard();
    };
}

#[test]
fn test_cli_help() {
    cli_serial!();
    let mut cmd = codescope_cmd();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("codescope"))
        .stdout(predicate::str::contains("init"))
        .stdout(predicate::str::contains("index"))
        .stdout(predicate::str::contains("search"))
        .stdout(predicate::str::contains("codescope trace callees --help"));
}

#[test]
fn test_cli_search_help_includes_dedupe() {
    cli_serial!();
    let mut cmd = codescope_cmd();
    cmd.args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--dedupe"))
        .stdout(predicate::str::contains("--no-dedupe"))
        .stdout(predicate::str::contains("--compact"))
        .stdout(predicate::str::contains("--excerpt-lines"));
}

#[test]
fn test_cli_trace_help_includes_output_flags() {
    cli_serial!();
    let mut cmd = codescope_cmd();
    cmd.args(["trace", "callers", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--pretty"))
        .stdout(predicate::str::contains("--compact"));

    let mut cmd = codescope_cmd();
    cmd.args(["trace", "callees", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--pretty"))
        .stdout(predicate::str::contains("--compact"));

    let mut cmd = codescope_cmd();
    cmd.args(["trace", "graph", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--pretty"))
        .stdout(predicate::str::contains("--compact"))
        .stdout(predicate::str::contains("--format"));
}

#[test]
fn test_cli_version() {
    cli_serial!();
    let mut cmd = codescope_cmd();
    cmd.arg("--version").assert().success();
}

#[test]
fn test_cli_init() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    // Check that .codescope directory was created
    assert!(temp_dir.path().join(".codescope").exists());
    assert!(temp_dir
        .path()
        .join(".codescope")
        .join("config.toml")
        .exists());
}

#[test]
fn test_cli_init_already_initialized() {
    cli_serial!();
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
    cli_serial!();
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
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    // Then check status
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("status")
        .assert()
        .success();
}

#[test]
fn test_cli_search_not_initialized() {
    cli_serial!();
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
    cli_serial!();
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
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    // Clean should succeed
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("clean")
        .assert()
        .success();
}

#[test]
fn test_cli_index_empty_project() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Initialize first
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    // Index should succeed even with no files
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();
}

#[test]
fn test_cli_index_with_file() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a sample file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function hello(): string { return 'world'; }",
    )
    .expect("Failed to write test file");

    // Initialize
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

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
    cli_serial!();
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
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    // Index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

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
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a sample file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function testFunction(): void { console.log('test'); }",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

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
fn test_cli_search_full_output_includes_snippet() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(
        temp_dir.path().join("test.ts"),
        r#"
export function testFunction(): void {
    console.log('test');
}
"#,
    )
    .expect("Failed to write test file");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("testFunction")
        .arg("--type")
        .arg("lexical")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file\""))
        .stdout(predicate::str::contains("\"snippet\""));
}

#[test]
fn test_cli_search_compact_output_omits_snippet() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(
        temp_dir.path().join("test.ts"),
        r#"
export function testFunction(): void {
    console.log('test');
}
"#,
    )
    .expect("Failed to write test file");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("testFunction")
        .arg("--type")
        .arg("lexical")
        .arg("--compact")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"file\""))
        .stdout(predicate::str::contains("\"snippet\"").not());
}

#[test]
fn test_cli_search_no_dedupe_flag_is_accepted() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(
        temp_dir.path().join("test.ts"),
        r#"
export function testFunction(): void {
    console.log('test');
}
"#,
    )
    .expect("Failed to write test file");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("search")
        .arg("testFunction")
        .arg("--type")
        .arg("lexical")
        .arg("--no-dedupe")
        .assert()
        .success();
}

#[test]
fn test_cli_search_excerpt_lines_truncates_jsonl_snippet() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    std::fs::write(
        temp_dir.path().join("test.ts"),
        r#"
export function longFunction(): void {
    console.log('line1');
    console.log('line2');
    console.log('line3');
    console.log('line4');
}
"#,
    )
    .expect("Failed to write test file");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    let assert = cmd
        .current_dir(temp_dir.path())
        .arg("search")
        .arg("longFunction")
        .arg("--type")
        .arg("lexical")
        .arg("--excerpt-lines")
        .arg("2")
        .assert()
        .success();

    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8 stdout");
    let first_line = stdout
        .lines()
        .find(|line| line.trim_start().starts_with('{'))
        .expect("at least one JSONL line");
    let parsed: serde_json::Value = serde_json::from_str(first_line).expect("valid JSONL");
    let snippet = parsed["snippet"]
        .as_str()
        .expect("compact mode should not be enabled");

    // 2 snippet lines + an "..." truncation line at most.
    assert!(snippet.lines().count() <= 3);
    assert!(snippet.trim_end().ends_with("..."));
}

#[test]
fn test_cli_search_excerpt_lines_rejects_zero() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .args(["search", "query", "--excerpt-lines", "0"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--excerpt-lines must be >= 1"));
}

#[test]
fn test_cli_search_top_k() {
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create multiple files
    for i in 0..5 {
        std::fs::write(
            temp_dir.path().join(format!("test{i}.ts")),
            format!("export function func{i}(): void {{}}"),
        )
        .expect("Failed to write test file");
    }

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

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
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create initial file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function initial(): void {}",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

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
    cli_serial!();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create a file
    std::fs::write(
        temp_dir.path().join("test.ts"),
        "export function test(): void {}",
    )
    .expect("Failed to write test file");

    // Initialize and index
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("init")
        .assert()
        .success();

    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .assert()
        .success();

    // Full re-index with --all
    let mut cmd = codescope_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("index")
        .arg("--all")
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed"));
}
