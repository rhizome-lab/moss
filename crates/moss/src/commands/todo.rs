//! Todo command - structured TODO.md editing without content loss

use std::fs;
use std::path::Path;

/// Parse TODO.md and extract items from a section
fn parse_section(content: &str, section: &str) -> Vec<(usize, String, bool)> {
    let mut items = Vec::new();
    let mut in_section = false;
    let section_header = format!("## {}", section);

    for (line_num, line) in content.lines().enumerate() {
        if line.starts_with("## ") {
            in_section = line == section_header;
            continue;
        }

        if in_section {
            // Parse list items: "1. item", "- item", "- [ ] item", "- [x] item"
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("- [x] ") {
                items.push((line_num, rest.to_string(), true));
            } else if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
                items.push((line_num, rest.to_string(), false));
            } else if let Some(rest) = trimmed.strip_prefix("- ") {
                items.push((line_num, rest.to_string(), false));
            } else if let Some(rest) = trimmed
                .strip_prefix(|c: char| c.is_ascii_digit())
                .and_then(|s| s.strip_prefix(". "))
            {
                items.push((line_num, rest.to_string(), false));
            }
        }
    }

    items
}

/// Add an item to a section
fn add_to_section(content: &str, section: &str, item: &str) -> String {
    let section_header = format!("## {}", section);
    let mut lines: Vec<&str> = content.lines().collect();
    let mut insert_at = None;

    for (i, line) in lines.iter().enumerate() {
        if *line == section_header {
            // Find end of section (next ## or numbered list end)
            for j in (i + 1)..lines.len() {
                if lines[j].starts_with("## ") {
                    insert_at = Some(j);
                    break;
                }
                // Insert after last numbered item
                if lines[j].trim().starts_with(|c: char| c.is_ascii_digit()) {
                    insert_at = Some(j + 1);
                }
            }
            if insert_at.is_none() {
                insert_at = Some(lines.len());
            }
            break;
        }
    }

    if let Some(pos) = insert_at {
        // Find the last numbered item to determine next number
        let mut next_num = 1;
        for j in (0..pos).rev() {
            let trimmed = lines[j].trim();
            if let Some(num_str) = trimmed
                .split('.')
                .next()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
            {
                if let Ok(num) = num_str.parse::<usize>() {
                    next_num = num + 1;
                    break;
                }
            }
            if trimmed.starts_with("## ") {
                break;
            }
        }

        let new_item = format!("{}. {}", next_num, item);
        lines.insert(pos, &new_item);

        // Can't return borrowed data, rebuild string
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            result.push_str(line);
            if i < lines.len() - 1 {
                result.push('\n');
            }
        }
        return result;
    }

    content.to_string()
}

/// Mark an item as done (by index in Next Up)
fn mark_done(content: &str, index: usize) -> Option<(String, String)> {
    let items = parse_section(content, "Next Up");
    if index == 0 || index > items.len() {
        return None;
    }

    let (line_num, item_text, _) = &items[index - 1];
    let lines: Vec<&str> = content.lines().collect();

    // Build new content without this line
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i != *line_num {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Renumber remaining items
    let result = renumber_section(&result, "Next Up");

    Some((result, item_text.clone()))
}

/// Renumber items in a section
fn renumber_section(content: &str, section: &str) -> String {
    let section_header = format!("## {}", section);
    let mut in_section = false;
    let mut item_num = 1;
    let mut result = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            in_section = line == section_header;
            item_num = 1;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_section {
            let trimmed = line.trim();
            // Check if it's a numbered item
            if let Some(rest) = trimmed
                .split_once('.')
                .filter(|(num, _)| num.chars().all(|c| c.is_ascii_digit()))
                .map(|(_, rest)| rest)
            {
                result.push_str(&format!("{}.{}\n", item_num, rest));
                item_num += 1;
                continue;
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') {
        result.pop();
    }

    result
}

/// Append completed item to CHANGELOG.md
fn append_to_changelog(changelog_path: &Path, item: &str) -> std::io::Result<()> {
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(changelog_path)?;

    writeln!(file, "- {}", item)?;
    Ok(())
}

/// Main command handler
pub fn cmd_todo(
    action: Option<&str>,
    item: Option<&str>,
    index: Option<usize>,
    full: bool,
    json: bool,
    root: &Path,
) -> i32 {
    let todo_path = root.join("TODO.md");
    let changelog_path = root.join("CHANGELOG.md");

    if !todo_path.exists() {
        eprintln!("No TODO.md found in {}", root.display());
        return 1;
    }

    let content = match fs::read_to_string(&todo_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading TODO.md: {}", e);
            return 1;
        }
    };

    match action {
        Some("add") => {
            let Some(item_text) = item else {
                eprintln!("Usage: moss todo add \"item text\"");
                return 1;
            };

            let new_content = add_to_section(&content, "Next Up", item_text);
            if let Err(e) = fs::write(&todo_path, &new_content) {
                eprintln!("Error writing TODO.md: {}", e);
                return 1;
            }

            if json {
                println!(
                    "{}",
                    serde_json::json!({"status": "added", "item": item_text})
                );
            } else {
                println!("Added to Next Up: {}", item_text);
            }
            0
        }

        Some("done") => {
            let Some(idx) = index else {
                eprintln!("Usage: moss todo done <index>");
                return 1;
            };

            match mark_done(&content, idx) {
                Some((new_content, completed_item)) => {
                    if let Err(e) = fs::write(&todo_path, &new_content) {
                        eprintln!("Error writing TODO.md: {}", e);
                        return 1;
                    }

                    // Append to CHANGELOG.md
                    if let Err(e) = append_to_changelog(&changelog_path, &completed_item) {
                        eprintln!("Warning: could not append to CHANGELOG.md: {}", e);
                    }

                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"status": "completed", "item": completed_item})
                        );
                    } else {
                        println!("Completed: {}", completed_item);
                    }
                    0
                }
                None => {
                    eprintln!("Invalid index: {}", idx);
                    1
                }
            }
        }

        None | Some("list") => {
            if full {
                if json {
                    println!("{}", serde_json::json!({"content": content}));
                } else {
                    print!("{}", content);
                }
                return 0;
            }

            let items = parse_section(&content, "Next Up");

            if json {
                let items_json: Vec<_> = items
                    .iter()
                    .enumerate()
                    .map(|(i, (_, text, done))| {
                        serde_json::json!({
                            "index": i + 1,
                            "text": text,
                            "done": done
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string(&items_json).unwrap());
            } else {
                if items.is_empty() {
                    println!("No items in Next Up");
                } else {
                    println!("## Next Up\n");
                    for (i, (_, text, done)) in items.iter().enumerate() {
                        let marker = if *done { "[x]" } else { "   " };
                        println!("{}  {}. {}", marker, i + 1, text);
                    }
                }
            }
            0
        }

        Some(other) => {
            eprintln!("Unknown action: {}", other);
            eprintln!("Usage: moss todo [add|done|list]");
            1
        }
    }
}
