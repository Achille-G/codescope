//! CLI integration tests for `codescope trace`

use assert_cmd::assert::OutputAssertExt;
#[allow(deprecated)]
use assert_cmd::cargo::cargo_bin;
use predicates::prelude::*;
use std::process::Command;
use tempfile::TempDir;

#[allow(deprecated)]
fn codescope_cmd() -> Command {
    Command::new(cargo_bin("codescope"))
}

#[test]
fn test_cli_trace_callers_and_graph() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    std::fs::create_dir_all(temp_dir.path().join("src")).expect("create src dir");

    std::fs::write(
        temp_dir.path().join("src/b.ts"),
        "export function foo(): void {}\n",
    )
    .expect("write b.ts");
    std::fs::write(
        temp_dir.path().join("src/a.ts"),
        "import { foo } from \"./b\";\nexport function caller(): void { foo(); }\n",
    )
    .expect("write a.ts");

    codescope_cmd()
        .current_dir(temp_dir.path())
        .args(["init"])
        .assert()
        .success();

    codescope_cmd()
        .current_dir(temp_dir.path())
        .args(["index", "--all"])
        .assert()
        .success();

    codescope_cmd()
        .current_dir(temp_dir.path())
        .args(["trace", "callers", "foo", "--file", "src/b.ts"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"type\":\"caller\""))
        .stdout(predicate::str::contains("\"file\":\"src/a.ts\""));

    codescope_cmd()
        .current_dir(temp_dir.path())
        .args([
            "trace", "graph", "caller", "--file", "src/a.ts", "--depth", "1", "--format", "dot",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("digraph call_graph"));
}
