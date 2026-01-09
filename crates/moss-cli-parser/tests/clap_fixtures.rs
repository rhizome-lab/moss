//! Integration tests using captured --help output from real CLIs.

use rhizome_moss_cli_parser::{parse_help, parse_help_with_format};

const EXAMPLE_HELP: &str = include_str!("../fixtures/clap/example.help");
const EXAMPLE_BUILD_HELP: &str = include_str!("../fixtures/clap/example-build.help");
const EXAMPLE_RUN_HELP: &str = include_str!("../fixtures/clap/example-run.help");

#[test]
fn test_detect_clap_format() {
    // Should auto-detect as clap
    let spec = parse_help(EXAMPLE_HELP).expect("should parse");
    assert_eq!(spec.name, Some("example".to_string()));
}

#[test]
fn test_parse_main_help() {
    let spec = parse_help_with_format(EXAMPLE_HELP, "clap").expect("should parse");

    // Check basic metadata
    assert_eq!(spec.name, Some("example".to_string()));
    assert_eq!(
        spec.description,
        Some("An example CLI tool for testing".to_string())
    );
    assert_eq!(spec.usage, Some("example [OPTIONS] [COMMAND]".to_string()));

    // Check commands (help is filtered out)
    assert_eq!(spec.commands.len(), 3);
    let cmd_names: Vec<_> = spec.commands.iter().map(|c| c.name.as_str()).collect();
    assert!(cmd_names.contains(&"build"));
    assert!(cmd_names.contains(&"run"));
    assert!(cmd_names.contains(&"clean"));
    assert!(!cmd_names.contains(&"help")); // help is filtered out

    // Check options (help and version are filtered out)
    assert_eq!(spec.options.len(), 3);

    // Find specific options
    let verbose = spec
        .options
        .iter()
        .find(|o| o.long == Some("--verbose".to_string()));
    assert!(verbose.is_some());
    let verbose = verbose.unwrap();
    assert_eq!(verbose.short, Some("-v".to_string()));
    assert_eq!(
        verbose.description,
        Some("Enable verbose output".to_string())
    );

    let config = spec
        .options
        .iter()
        .find(|o| o.long == Some("--config".to_string()));
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(config.short, Some("-c".to_string()));
    assert_eq!(config.value, Some("<FILE>".to_string()));

    let port = spec
        .options
        .iter()
        .find(|o| o.long == Some("--port".to_string()));
    assert!(port.is_some());
    let port = port.unwrap();
    assert_eq!(port.short, Some("-p".to_string()));
    assert_eq!(port.value, Some("<PORT>".to_string()));
    assert_eq!(port.default, Some("8080".to_string()));
}

#[test]
fn test_parse_build_subcommand_help() {
    let spec = parse_help_with_format(EXAMPLE_BUILD_HELP, "clap").expect("should parse");

    assert_eq!(spec.description, Some("Build the project".to_string()));
    assert_eq!(spec.usage, Some("example build [OPTIONS]".to_string()));

    // Check options (help is filtered out)
    assert_eq!(spec.options.len(), 2);

    let release = spec
        .options
        .iter()
        .find(|o| o.long == Some("--release".to_string()));
    assert!(release.is_some());
    let release = release.unwrap();
    assert_eq!(release.short, Some("-r".to_string()));
    assert_eq!(
        release.description,
        Some("Build in release mode".to_string())
    );

    let target = spec
        .options
        .iter()
        .find(|o| o.long == Some("--target".to_string()));
    assert!(target.is_some());
    let target = target.unwrap();
    assert_eq!(target.short, Some("-t".to_string()));
    assert_eq!(target.value, Some("<DIR>".to_string()));
}

#[test]
fn test_parse_run_subcommand_help() {
    let spec = parse_help_with_format(EXAMPLE_RUN_HELP, "clap").expect("should parse");

    assert_eq!(spec.description, Some("Run the project".to_string()));
    assert_eq!(spec.usage, Some("example run [ARGS]...".to_string()));

    // No non-help options
    assert_eq!(spec.options.len(), 0);
}

#[test]
fn test_command_descriptions() {
    let spec = parse_help_with_format(EXAMPLE_HELP, "clap").expect("should parse");

    let build = spec.commands.iter().find(|c| c.name == "build").unwrap();
    assert_eq!(build.description, Some("Build the project".to_string()));

    let run = spec.commands.iter().find(|c| c.name == "run").unwrap();
    assert_eq!(run.description, Some("Run the project".to_string()));

    let clean = spec.commands.iter().find(|c| c.name == "clean").unwrap();
    assert_eq!(clean.description, Some("Clean build artifacts".to_string()));
}
