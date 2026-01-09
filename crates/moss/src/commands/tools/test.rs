//! Test command - run native test runners.

use std::path::Path;

use rhizome_moss_tools::test_runners::{all_runners, detect_test_runner, get_runner};

/// Run tests with auto-detected or specified runner.
pub fn cmd_test_run(root: Option<&Path>, runner: Option<&str>, args: &[String]) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    let test_runner = if let Some(name) = runner {
        // Find specific runner by name
        get_runner(name)
    } else {
        // Auto-detect
        detect_test_runner(root)
    };

    let Some(test_runner) = test_runner else {
        eprintln!("No test runner detected for this project.");
        eprintln!("Supported: cargo (Rust), go (Go), bun/npm (JS/TS), pytest (Python)");
        return 1;
    };

    let info = test_runner.info();
    eprintln!("Running tests with {}...", info.name);

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    match test_runner.run(root, &args_refs) {
        Ok(result) => {
            if result.success() {
                0
            } else {
                result.status.code().unwrap_or(1)
            }
        }
        Err(e) => {
            eprintln!("Failed to run tests: {}", e);
            1
        }
    }
}

/// List available test runners.
pub fn cmd_test_list(root: Option<&Path>) -> i32 {
    let root = root.unwrap_or_else(|| Path::new("."));

    println!("Available test runners:\n");

    for runner in all_runners() {
        let info = runner.info();
        let available = runner.is_available();
        let score = runner.detect(root);

        let status = if !available {
            "(not installed)"
        } else if score > 0.0 {
            "(detected)"
        } else {
            ""
        };

        println!("  {:10} - {} {}", info.name, info.description, status);
    }

    0
}
