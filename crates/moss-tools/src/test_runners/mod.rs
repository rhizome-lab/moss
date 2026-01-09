//! Test runner adapters for different ecosystems.
//!
//! Each test runner detects whether it applies to a project and runs the native test command.
//!
//! # Extensibility
//!
//! Users can register custom test runners via [`register()`]:
//!
//! ```ignore
//! use rhizome_moss_tools::test_runners::{TestRunner, TestRunnerInfo, TestResult, register};
//! use std::path::Path;
//!
//! struct MyTestRunner;
//!
//! impl TestRunner for MyTestRunner {
//!     fn info(&self) -> TestRunnerInfo { /* ... */ }
//!     fn is_available(&self) -> bool { /* ... */ }
//!     fn detect(&self, root: &Path) -> f32 { /* ... */ }
//!     fn run(&self, root: &Path, args: &[&str]) -> std::io::Result<TestResult> { /* ... */ }
//! }
//!
//! // Register before first use
//! register(&MyTestRunner);
//! ```

mod bun;
mod cargo;
mod go;
mod npm;
mod pytest;

pub use bun::BunTest;
pub use cargo::CargoTest;
pub use go::GoTest;
pub use npm::NpmTest;
pub use pytest::Pytest;

use std::path::Path;
use std::process::ExitStatus;
use std::sync::{OnceLock, RwLock};

/// Information about a test runner.
#[derive(Debug, Clone)]
pub struct TestRunnerInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// Result of running tests.
#[derive(Debug)]
pub struct TestResult {
    pub runner: String,
    pub status: ExitStatus,
}

impl TestResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }
}

/// A test runner that can detect and run tests for a project type.
pub trait TestRunner: Send + Sync {
    /// Info about this test runner.
    fn info(&self) -> TestRunnerInfo;

    /// Check if this test runner is available (binary exists).
    fn is_available(&self) -> bool;

    /// Detect if this runner applies to the project. Returns confidence 0.0-1.0.
    fn detect(&self, root: &Path) -> f32;

    /// Run tests, streaming output to stdout/stderr.
    fn run(&self, root: &Path, args: &[&str]) -> std::io::Result<TestResult>;
}

/// Global registry of test runner plugins.
static RUNNERS: RwLock<Vec<&'static dyn TestRunner>> = RwLock::new(Vec::new());
static INITIALIZED: OnceLock<()> = OnceLock::new();

/// Register a custom test runner plugin.
///
/// Call this before any detection operations to add custom runners.
/// Built-in runners are registered automatically on first use.
pub fn register(runner: &'static dyn TestRunner) {
    RUNNERS.write().unwrap().push(runner);
}

/// Initialize built-in runners (called automatically on first use).
fn init_builtin() {
    INITIALIZED.get_or_init(|| {
        let mut runners = RUNNERS.write().unwrap();
        static CARGO: CargoTest = CargoTest;
        static GO: GoTest = GoTest;
        static BUN: BunTest = BunTest;
        static NPM: NpmTest = NpmTest;
        static PYTEST: Pytest = Pytest;
        runners.push(&CARGO);
        runners.push(&GO);
        runners.push(&BUN);
        runners.push(&NPM);
        runners.push(&PYTEST);
    });
}

/// Get a test runner by name from the global registry.
pub fn get_runner(name: &str) -> Option<&'static dyn TestRunner> {
    init_builtin();
    RUNNERS
        .read()
        .unwrap()
        .iter()
        .find(|r| r.info().name == name)
        .copied()
}

/// List all available runner names from the global registry.
pub fn list_runners() -> Vec<&'static str> {
    init_builtin();
    RUNNERS
        .read()
        .unwrap()
        .iter()
        .map(|r| r.info().name)
        .collect()
}

/// Get all runners from the global registry.
pub fn all_runners() -> Vec<&'static dyn TestRunner> {
    init_builtin();
    RUNNERS.read().unwrap().clone()
}

/// Get all available test runners (returns boxed runners for backwards compatibility).
pub fn all_test_runners() -> Vec<Box<dyn TestRunner>> {
    vec![
        Box::new(CargoTest::new()),
        Box::new(GoTest::new()),
        Box::new(BunTest::new()),
        Box::new(NpmTest::new()),
        Box::new(Pytest::new()),
    ]
}

/// Find the best test runner for a project using the global registry.
pub fn detect_test_runner(root: &Path) -> Option<&'static dyn TestRunner> {
    init_builtin();
    let runners = RUNNERS.read().unwrap();

    let mut best: Option<(&'static dyn TestRunner, f32)> = None;

    for runner in runners.iter() {
        if !runner.is_available() {
            continue;
        }

        let score = runner.detect(root);
        if score > 0.0 {
            if best.is_none() || score > best.unwrap().1 {
                best = Some((*runner, score));
            }
        }
    }

    best.map(|(runner, _)| runner)
}
