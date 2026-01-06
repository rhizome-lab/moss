//! Integration tests using captured --help output from click CLIs.

use moss_cli_parser::{parse_help, parse_help_with_format};

const EXAMPLE_HELP: &str = include_str!("../fixtures/click/example.help");

#[test]
fn test_detect_click_format() {
    let spec = parse_help(EXAMPLE_HELP).expect("should parse");
    assert_eq!(spec.name, Some("example".to_string()));
}

#[test]
fn test_parse_main_help() {
    let spec = parse_help_with_format(EXAMPLE_HELP, "click").expect("should parse");

    assert_eq!(spec.name, Some("example".to_string()));
    assert_eq!(
        spec.description,
        Some("An example CLI tool for testing.".to_string())
    );

    // Check commands
    assert_eq!(spec.commands.len(), 3);
    let cmd_names: Vec<_> = spec.commands.iter().map(|c| c.name.as_str()).collect();
    assert!(cmd_names.contains(&"build"));
    assert!(cmd_names.contains(&"run"));
    assert!(cmd_names.contains(&"clean"));

    // Check options (help/version filtered out)
    assert_eq!(spec.options.len(), 3);

    let verbose = spec
        .options
        .iter()
        .find(|o| o.long == Some("--verbose".to_string()));
    assert!(verbose.is_some());
}
