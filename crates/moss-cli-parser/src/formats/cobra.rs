//! Parser for Go cobra-style --help output.
//!
//! Cobra format characteristics:
//! - Description on first line (no Usage: prefix initially)
//! - `Usage:` section with `<name> [command]`
//! - `Available Commands:` section (not just "Commands:")
//! - `Flags:` section (not "Options:")
//! - Footer: `Use "<name> [command] --help" for more information`

use super::CliFormat;
use crate::{CliCommand, CliOption, CliSpec};
use regex::Regex;

/// Parser for Go cobra-style CLI help output.
pub struct CobraFormat;

impl CliFormat for CobraFormat {
    fn name(&self) -> &'static str {
        "cobra"
    }

    fn detect(&self, help_text: &str) -> f64 {
        let mut score: f64 = 0.0;

        // Check for "Available Commands:" (cobra-specific)
        if help_text.contains("Available Commands:") {
            score += 0.4;
        }

        // Check for "Flags:" section (cobra uses Flags, not Options)
        if help_text.contains("\nFlags:\n") {
            score += 0.3;
        }

        // Check for cobra-style footer
        if help_text.contains("for more information about a command") {
            score += 0.2;
        }

        // Check for "Usage:" section
        if help_text.contains("\nUsage:\n") {
            score += 0.1;
        }

        // Negative: "Options:" is more like clap/click
        if help_text.contains("\nOptions:\n") {
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

        // Parse description (lines until "Usage:" section)
        let mut description_lines = Vec::new();
        while i < lines.len() {
            let line = lines[i];
            if line == "Usage:" || is_section_header(line) {
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

            if line == "Usage:" {
                i += 1;
                // Usage content is indented
                while i < lines.len() && lines[i].starts_with("  ") {
                    let usage = lines[i].trim();
                    spec.usage = Some(usage.to_string());
                    // Extract name from usage
                    if let Some(name) = usage.split_whitespace().next() {
                        spec.name = Some(name.to_string());
                    }
                    i += 1;
                }
            } else if line == "Available Commands:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(cmd) = parse_command_line(lines[i]) {
                        spec.commands.push(cmd);
                    }
                    i += 1;
                }
            } else if line == "Flags:" || line == "Global Flags:" {
                i += 1;
                while i < lines.len() && !is_section_header(lines[i]) {
                    if let Some(opt) = parse_flag_line(lines[i]) {
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

fn parse_command_line(line: &str) -> Option<CliCommand> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let re = Regex::new(r"^(\S+)\s{2,}(.*)$").unwrap();
    if let Some(caps) = re.captures(trimmed) {
        let name = caps.get(1)?.as_str().to_string();
        let description = caps.get(2).map(|m| m.as_str().to_string());

        // Skip help/completion commands
        if name == "help" || name == "completion" {
            return None;
        }

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

/// Parse cobra flag line.
/// Format: "-c, --config string  Description" or "    --version  description"
fn parse_flag_line(line: &str) -> Option<CliOption> {
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

    // Pattern: "-c, --config string  Description" or "--version  description"
    // Cobra puts type after flag name: "--config string" not "--config <string>"
    let re = Regex::new(r"^(-\w)?(?:,\s*)?(--[\w-]+)(?:\s+(string|int|bool))?\s{2,}(.*)$").unwrap();

    if let Some(caps) = re.captures(trimmed) {
        opt.short = caps.get(1).map(|m| m.as_str().to_string());
        opt.long = caps.get(2).map(|m| m.as_str().to_string());
        opt.value = caps.get(3).map(|m| format!("<{}>", m.as_str()));
        opt.description = caps.get(4).map(|m| m.as_str().to_string());

        // Check for default value in description: (default X)
        if let Some(ref desc) = opt.description {
            if let Some(start) = desc.find("(default") {
                if let Some(end) = desc[start..].find(')') {
                    let default = desc[start + 8..start + end].trim().to_string();
                    opt.default = Some(default);
                }
            }
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
    fn test_detect_cobra() {
        let help = r#"A tool

Usage:
  example [command]

Available Commands:
  build  Build something

Flags:
  -h, --help  help for example
"#;
        let format = CobraFormat;
        assert!(format.detect(help) > 0.5);
    }

    #[test]
    fn test_parse_flag_with_type() {
        let help = r#"A tool

Usage:
  example [command]

Flags:
  -c, --config string  Config file
"#;
        let spec = CobraFormat.parse(help).unwrap();
        assert_eq!(spec.options.len(), 1);
        assert_eq!(spec.options[0].value, Some("<string>".to_string()));
    }
}
