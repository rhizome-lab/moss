//! Integration tests using captured --help output from argparse CLIs.

use rhizome_moss_cli_parser::{parse_help, parse_help_with_format};

const EXAMPLE_HELP: &str = include_str!("../fixtures/argparse/example.help");

#[test]
fn test_detect_argparse_format() {
    // Should auto-detect as argparse
    let spec = parse_help(EXAMPLE_HELP).expect("should parse");
    assert_eq!(spec.name, Some("example".to_string()));
}

#[test]
fn test_parse_main_help() {
    let spec = parse_help_with_format(EXAMPLE_HELP, "argparse").expect("should parse");

    // Check basic metadata
    assert_eq!(spec.name, Some("example".to_string()));
    assert_eq!(
        spec.description,
        Some("An example CLI tool for testing".to_string())
    );
    assert!(spec.usage.is_some());

    // Check commands
    assert_eq!(spec.commands.len(), 3);
    let cmd_names: Vec<_> = spec.commands.iter().map(|c| c.name.as_str()).collect();
    assert!(cmd_names.contains(&"build"));
    assert!(cmd_names.contains(&"run"));
    assert!(cmd_names.contains(&"clean"));

    // Check options (help filtered out)
    assert_eq!(spec.options.len(), 3);

    // Find specific options
    let verbose = spec
        .options
        .iter()
        .find(|o| o.long == Some("--verbose".to_string()));
    assert!(verbose.is_some());
    let verbose = verbose.unwrap();
    assert_eq!(verbose.short, Some("-v".to_string()));

    let config = spec
        .options
        .iter()
        .find(|o| o.long == Some("--config".to_string()));
    assert!(config.is_some());
    let config = config.unwrap();
    assert_eq!(config.value, Some("<FILE>".to_string()));

    let port = spec
        .options
        .iter()
        .find(|o| o.long == Some("--port".to_string()));
    assert!(port.is_some());
    let port = port.unwrap();
    assert_eq!(port.default, Some("8080".to_string()));
}

#[test]
fn test_command_descriptions() {
    let spec = parse_help_with_format(EXAMPLE_HELP, "argparse").expect("should parse");

    let build = spec.commands.iter().find(|c| c.name == "build").unwrap();
    assert_eq!(build.description, Some("Build the project".to_string()));

    let run = spec.commands.iter().find(|c| c.name == "run").unwrap();
    assert_eq!(run.description, Some("Run the project".to_string()));

    let clean = spec.commands.iter().find(|c| c.name == "clean").unwrap();
    assert_eq!(clean.description, Some("Clean build artifacts".to_string()));
}
