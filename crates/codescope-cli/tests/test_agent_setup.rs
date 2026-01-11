//! Integration tests for agent-setup command

use assert_cmd::assert::OutputAssertExt;
#[allow(deprecated)]
use assert_cmd::cargo::cargo_bin;
use std::fs::{self, File};
use std::io::Write;
use std::process::Command;
use tempfile::TempDir;

#[test]
#[allow(deprecated)]
fn test_agent_setup_no_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    Command::new(cargo_bin("codescope"))
        .args(["agent-setup"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "No agent configuration files found",
        ));
}

#[test]
#[allow(deprecated)]
fn test_agent_setup_idempotent() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create CLAUDE.md with existing codescope instructions (full content)
    let claude_md = temp_dir.path().join("CLAUDE.md");
    let mut file = File::create(&claude_md).expect("Failed to create CLAUDE.md");
    file.write_all(
        b"## codescope - Semantic Code Search\n\nAlready configured.\ncodescope search\n",
    )
    .expect("Failed to write");

    // Run agent-setup
    Command::new(cargo_bin("codescope"))
        .args(["agent-setup"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Already configured"));

    // Read the file - should have codescope instructions unchanged
    let content = fs::read_to_string(&claude_md).expect("Failed to read");
    let count = content.matches("codescope search").count();
    assert_eq!(count, 1, "Should have exactly one codescope search mention");
}

#[test]
#[allow(deprecated)]
fn test_agent_setup_adds_instructions() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create .cursorrules without codescope instructions
    let cursorrules = temp_dir.path().join(".cursorrules");
    let mut file = File::create(&cursorrules).expect("Failed to create .cursorrules");
    file.write_all(b"# Cursor rules\n\nSome existing rules.\n")
        .expect("Failed to write");

    // Run agent-setup
    Command::new(cargo_bin("codescope"))
        .args(["agent-setup"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Found: .cursorrules"))
        .stdout(predicates::str::contains("Added codescope instructions"));

    // Read the file - should have codescope instructions
    let content = fs::read_to_string(&cursorrules).expect("Failed to read");
    assert!(content.contains("## codescope - Semantic Code Search"));
    assert!(content.contains("codescope search"));
}

#[test]
#[allow(deprecated)]
fn test_agent_setup_multiple_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Create multiple agent files
    for file_name in [".cursorrules", ".windsurfrules", "CLAUDE.md"] {
        let path = temp_dir.path().join(file_name);
        let mut file =
            File::create(&path).unwrap_or_else(|_| panic!("Failed to create {file_name}"));
        file.write_all(format!("# {file_name}\n\n").as_bytes())
            .unwrap_or_else(|_| panic!("Failed to write to {file_name}"));
    }

    // Run agent-setup
    Command::new(cargo_bin("codescope"))
        .args(["agent-setup"])
        .current_dir(temp_dir.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("Found: .cursorrules"))
        .stdout(predicates::str::contains("Found: .windsurfrules"))
        .stdout(predicates::str::contains("Found: CLAUDE.md"))
        .stdout(predicates::str::contains("Updated 3 file(s)"));
}
