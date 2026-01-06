//! Parser for Python click-style --help output.
//!
//! Click format characteristics:
//! - `Usage: <name> [OPTIONS] COMMAND [ARGS]...`
//! - Description indented below Usage line
//! - `Options:` section with `-s, --long VALUE  Description`
//! - `Commands:` section
//! - Help option shows "Show this message and exit."

use super::CliFormat;
use crate::{CliCommand, CliOption, CliSpec};
use regex::Regex;

/// Parser for Python click-style CLI help output.
pub struct ClickFormat;

impl CliFormat for ClickFormat {
    fn name(&self) -> &'static str {
        "click"
    }

    fn detect(&self, help_text: &str) -> f64 {
        let mut score: f64 = 0.0;

        // Check for "Usage:" line
        if help_text.contains("Usage:") {
            score += 0.2;
        }

        // Check for click-specific help text
        if help_text.contains("Show this message and exit.") {
            score += 0.4;
        }

        // Check for click-specific version text
        if help_text.contains("Show the version and exit.") {
            score += 0.2;
        }

        // Check for "Options:" section
        if help_text.contains("\nOptions:\n") {
            score += 0.1;
        }

        // Check for "Commands:" section
        if help_text.contains("\nCommands:\n") {
            score += 0.1;
        }

        score.min(1.0)
    }

    fn parse(&self, help_text: &str) -> Result<CliSpec, String> {
        let mut spec = CliSpec::default();
        let lines: Vec<&str> = help_text.lines().collect();

        if lines.is_empty() {
            return Err("Empty help text".to_string());
        }

        let mut i = 0;

        // Parse "Usage: <name> ..." line
        if let Some(first_line) = lines.first() {
            if let Some(usage) = first_line.strip_prefix("Usage:") {
                let usage = usage.trim();
                spec.usage = Some(usage.to_string());
                // Extract name (could be script.py or just name)
                if let Some(name) = usage.split_whitespace().next() {
                    // Remove .py extension if present
                    let name = name.strip_suffix(".py").unwrap_or(name);
                    spec.name = Some(name.to_string());
                }
            }
            i += 1;
        }

        // Parse description (indented lines after Usage until section header)
        let mut description_lines = Vec::new();
        while i < lines.len() {
            let line = lines[i];
            if is_section_header(line) {
                break;
            }
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                description_lines.push(trimmed);
            }
            i += 1;
        }
        if !description_lines.is_empty() {
            spec.description = Some(description_lines.join(" "));
        }

        // Parse sections
        while i < lines.len() {
            let line = lines[i];

            if line == "Options:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(opt) = parse_option_line(lines[i]) {
                        spec.options.push(opt);
                    }
                    i += 1;
                }
            } else if line == "Commands:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(cmd) = parse_command_line(lines[i]) {
                        spec.commands.push(cmd);
                    }
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        Ok(spec)
    }
}

fn is_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('-')
        && !trimmed.starts_with(' ')
        && trimmed.ends_with(':')
}

fn parse_option_line(line: &str) -> Option<CliOption> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.starts_with('-') {
        return None;
    }

    let mut opt = CliOption {
        short: None,
        long: None,
        value: None,
        description: None,
        default: None,
        required: false,
        env: None,
    };

    // Pattern: "-s, --long VALUE  Description" or "--long VALUE  Description"
    let re = Regex::new(r"^(-\w)?(?:,\s*)?(--[\w-]+)?(?:\s+([A-Z_]+))?\s{2,}(.*)$").unwrap();

    if let Some(caps) = re.captures(trimmed) {
        opt.short = caps.get(1).map(|m| m.as_str().to_string());
        opt.long = caps.get(2).map(|m| m.as_str().to_string());
        opt.value = caps.get(3).map(|m| format!("<{}>", m.as_str()));
        opt.description = caps.get(4).map(|m| m.as_str().to_string());

        // Skip help/version
        if opt.long == Some("--help".to_string()) || opt.long == Some("--version".to_string()) {
            return None;
        }

        Some(opt)
    } else {
        None
    }
}

fn parse_command_line(line: &str) -> Option<CliCommand> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let re = Regex::new(r"^(\S+)\s{2,}(.*)$").unwrap();
    if let Some(caps) = re.captures(trimmed) {
        let name = caps.get(1)?.as_str().to_string();
        let description = caps.get(2).map(|m| m.as_str().to_string());

        Some(CliCommand {
            name,
            description,
            aliases: Vec::new(),
            options: Vec::new(),
            subcommands: Vec::new(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_click() {
        let help = r#"Usage: example [OPTIONS]

Options:
  --help  Show this message and exit.
"#;
        let format = ClickFormat;
        assert!(format.detect(help) > 0.5);
    }

    #[test]
    fn test_parse_name_from_usage() {
        let help = "Usage: example.py [OPTIONS]\n\n  A tool.\n\nOptions:\n  --help  Show this message and exit.\n";
        let spec = ClickFormat.parse(help).unwrap();
        assert_eq!(spec.name, Some("example".to_string()));
    }
}
