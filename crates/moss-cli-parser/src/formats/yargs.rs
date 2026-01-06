//! Parser for Node.js yargs-style --help output.
//!
//! Yargs format characteristics:
//! - First line is usage: `<name> <command> [options]` (no "Usage:" prefix)
//! - `Commands:` section with `<name> <cmd>  Description`
//! - `Options:` section with type annotations: `[boolean]`, `[string]`, `[number]`
//! - Default values shown as `[default: X]`

use super::CliFormat;
use crate::{CliCommand, CliOption, CliSpec};
use regex::Regex;

/// Parser for Node.js yargs-style CLI help output.
pub struct YargsFormat;

impl CliFormat for YargsFormat {
    fn name(&self) -> &'static str {
        "yargs"
    }

    fn detect(&self, help_text: &str) -> f64 {
        let mut score: f64 = 0.0;

        // Check for yargs-style type annotations (strong signals)
        if help_text.contains("[boolean]") {
            score += 0.4;
        }
        if help_text.contains("[string]") {
            score += 0.3;
        }
        if help_text.contains("[number]") {
            score += 0.3;
        }

        // Check for yargs-style default
        if help_text.contains("[default:") {
            score += 0.2;
        }

        // Check for Commands/Options sections
        if help_text.contains("\nCommands:\n") {
            score += 0.1;
        }
        if help_text.contains("\nOptions:\n") {
            score += 0.1;
        }

        // Negative: "Usage:" prefix is more like clap/click
        if help_text.starts_with("Usage:") {
            score -= 0.3;
        }

        score.clamp(0.0, 1.0)
    }

    fn parse(&self, help_text: &str) -> Result<CliSpec, String> {
        let mut spec = CliSpec::default();
        let lines: Vec<&str> = help_text.lines().collect();

        if lines.is_empty() {
            return Err("Empty help text".to_string());
        }

        let mut i = 0;

        // First line is usage (no prefix)
        if let Some(first_line) = lines.first() {
            let usage = first_line.trim();
            spec.usage = Some(usage.to_string());
            // Extract name from usage
            if let Some(name) = usage.split_whitespace().next() {
                spec.name = Some(name.to_string());
            }
            i += 1;
        }

        // Parse description (lines until section header)
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

            if line == "Commands:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(cmd) = parse_command_line(lines[i], spec.name.as_deref()) {
                        spec.commands.push(cmd);
                    }
                    i += 1;
                }
            } else if line == "Options:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(opt) = parse_option_line(lines[i]) {
                        spec.options.push(opt);
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

/// Parse yargs command line: "example build  Description"
fn parse_command_line(line: &str, prog_name: Option<&str>) -> Option<CliCommand> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Yargs commands are "progname cmdname  Description"
    let re = Regex::new(r"^(\S+)\s+(\S+)(?:\s+\[[^\]]+\])?\s{2,}(.*)$").unwrap();
    if let Some(caps) = re.captures(trimmed) {
        let prefix = caps.get(1)?.as_str();
        let cmd_name = caps.get(2)?.as_str().to_string();
        let description = caps.get(3).map(|m| m.as_str().to_string());

        // Verify prefix matches program name
        if let Some(name) = prog_name {
            if prefix != name {
                return None;
            }
        }

        Some(CliCommand {
            name: cmd_name,
            description,
            aliases: Vec::new(),
            options: Vec::new(),
            subcommands: Vec::new(),
        })
    } else {
        None
    }
}

/// Parse yargs option line with type annotations.
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

    // Pattern: "-v, --verbose  Description  [boolean]"
    // Or: "    --help  Description  [boolean]"
    let re = Regex::new(r"^(-\w)?(?:,\s*)?(--[\w-]+)\s{2,}(.+)$").unwrap();

    if let Some(caps) = re.captures(trimmed) {
        opt.short = caps.get(1).map(|m| m.as_str().to_string());
        opt.long = caps.get(2).map(|m| m.as_str().to_string());

        let rest = caps.get(3).map(|m| m.as_str()).unwrap_or("");

        // Parse type annotation and default from the rest
        // Format: "Description  [type] [default: X]" or "Description  [type]"
        if let Some(bracket_start) = rest.rfind('[') {
            let description = rest[..bracket_start].trim();
            opt.description = if description.is_empty() {
                None
            } else {
                Some(description.to_string())
            };

            // Extract type and default from bracketed parts
            let brackets = &rest[bracket_start..];
            if brackets.contains("[default:") {
                if let Some(start) = brackets.find("[default:") {
                    if let Some(end) = brackets[start..].find(']') {
                        let default = brackets[start + 9..start + end].trim().to_string();
                        opt.default = Some(default);
                    }
                }
            }

            // Infer value type from annotations
            if brackets.contains("[string]") {
                opt.value = Some("<string>".to_string());
            } else if brackets.contains("[number]") {
                opt.value = Some("<number>".to_string());
            }
        } else {
            opt.description = Some(rest.to_string());
        }

        // Skip help/version
        if opt.long == Some("--help".to_string()) || opt.long == Some("--version".to_string()) {
            return None;
        }

        Some(opt)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_yargs() {
        let help = r#"example <command> [options]

Commands:
  example build  Build

Options:
  -v, --verbose  Enable verbose  [boolean]
"#;
        let format = YargsFormat;
        assert!(format.detect(help) > 0.5);
    }

    #[test]
    fn test_parse_option_with_default() {
        let help = r#"example [options]

Options:
  -p, --port  Port number  [number] [default: 8080]
"#;
        let spec = YargsFormat.parse(help).unwrap();
        assert_eq!(spec.options.len(), 1);
        assert_eq!(spec.options[0].default, Some("8080".to_string()));
    }
}
