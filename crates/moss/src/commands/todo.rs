//! Todo command - structured TODO.md editing without content loss
//!
//! Detects common TODO.md formats automatically:
//! - Section headers: `##`, `#`, `###` with common names
//! - Item formats: checkboxes `- [ ]`, numbers `1.`, bullets `-`
//! - Preserves user's existing format when adding items

use std::fs;
use std::path::Path;

use clap::{Args, Subcommand};

use crate::config::{MossConfig, TodoConfig};

#[derive(Subcommand)]
pub enum TodoAction {
    /// List items (primary section by default, or filtered)
    List {
        #[command(flatten)]
        filter: ListFilter,
    },
    /// Add an item to the primary section
    Add {
        /// Item text to add
        text: String,
        /// Target section (default: primary section)
        #[arg(short, long)]
        section: Option<String>,
    },
    /// Mark an item as done (fuzzy text match)
    Done {
        /// Text to match (case-insensitive substring)
        query: String,
        /// Target section (default: primary section)
        #[arg(short, long)]
        section: Option<String>,
    },
    /// Remove an item (fuzzy text match)
    Rm {
        /// Text to match (case-insensitive substring)
        query: String,
        /// Target section (default: primary section)
        #[arg(short, long)]
        section: Option<String>,
    },
    /// Remove all completed items
    Clean,
}

#[derive(Args, Default)]
pub struct ListFilter {
    /// Show full raw TODO.md content
    #[arg(long)]
    pub raw: bool,

    /// Show only completed items
    #[arg(long, conflicts_with = "pending")]
    pub done: bool,

    /// Show only pending items (default)
    #[arg(long, conflicts_with = "done")]
    pub pending: bool,

    /// Show all items regardless of status
    #[arg(short, long, conflicts_with_all = ["done", "pending"])]
    pub all: bool,

    /// Filter to specific section (fuzzy match)
    #[arg(short, long)]
    pub section: Option<String>,
}

/// Detected item format in a section
#[derive(Debug, Clone, Copy, PartialEq)]
enum ItemFormat {
    Checkbox, // - [ ] item / - [x] item
    Numbered, // 1. item
    Bullet,   // - item
    Asterisk, // * item
    Plain,    // just text lines
}

/// A detected section in the TODO file
#[derive(Debug)]
struct Section {
    name: String,
    path: String, // full path like "Backlog/Language Support"
    header_line: usize,
    header_level: usize, // number of # chars
    items: Vec<Item>,
    format: ItemFormat,
}

/// An item within a section
#[derive(Debug)]
struct Item {
    line_num: usize,
    text: String,
    done: bool,
    raw_line: String,
}

/// Priority names for the "primary" section (checked in order)
const PRIMARY_SECTION_NAMES: &[&str] = &[
    "next up",
    "next",
    "todo",
    "tasks",
    "in progress",
    "current",
    "active",
];

/// Parse the entire TODO file structure
fn parse_todo(content: &str) -> Vec<Section> {
    let lines: Vec<&str> = content.lines().collect();
    let mut sections = Vec::new();
    let mut current_section: Option<Section> = None;
    // Track parent names at each level for building paths
    let mut parent_stack: Vec<(usize, String)> = Vec::new(); // (level, name)

    for (line_num, line) in lines.iter().enumerate() {
        // Detect section headers
        if let Some((level, name)) = parse_header(line) {
            // Save previous section
            if let Some(mut section) = current_section.take() {
                section.format = detect_format(&section.items);
                sections.push(section);
            }

            // Update parent stack - pop anything at same or deeper level
            while parent_stack
                .last()
                .map(|(l, _)| *l >= level)
                .unwrap_or(false)
            {
                parent_stack.pop();
            }

            // Build path from parent stack
            let path = if parent_stack.is_empty() {
                name.clone()
            } else {
                let parent_path: String = parent_stack
                    .iter()
                    .map(|(_, n)| n.as_str())
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{}/{}", parent_path, name)
            };

            // Push this section onto the stack
            parent_stack.push((level, name.clone()));

            current_section = Some(Section {
                name,
                path,
                header_line: line_num,
                header_level: level,
                items: Vec::new(),
                format: ItemFormat::Plain,
            });
            continue;
        }

        // Parse items within current section
        if let Some(ref mut section) = current_section {
            if let Some(item) = parse_item(line, line_num) {
                section.items.push(item);
            }
        }
    }

    // Don't forget the last section
    if let Some(mut section) = current_section {
        section.format = detect_format(&section.items);
        sections.push(section);
    }

    sections
}

/// Parse a markdown header, returns (level, name)
fn parse_header(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('#') {
        return None;
    }

    let level = trimmed.chars().take_while(|&c| c == '#').count();
    let name = trimmed[level..].trim().to_string();

    if name.is_empty() {
        return None;
    }

    Some((level, name))
}

/// Parse a line as an item
fn parse_item(line: &str, line_num: usize) -> Option<Item> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Checkbox: - [ ] or - [x]
    if let Some(rest) = trimmed
        .strip_prefix("- [x] ")
        .or_else(|| trimmed.strip_prefix("- [X] "))
    {
        return Some(Item {
            line_num,
            text: rest.to_string(),
            done: true,
            raw_line: line.to_string(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
        return Some(Item {
            line_num,
            text: rest.to_string(),
            done: false,
            raw_line: line.to_string(),
        });
    }

    // Bullet: - item
    if let Some(rest) = trimmed.strip_prefix("- ") {
        return Some(Item {
            line_num,
            text: rest.to_string(),
            done: false,
            raw_line: line.to_string(),
        });
    }

    // Asterisk: * item
    if let Some(rest) = trimmed.strip_prefix("* ") {
        return Some(Item {
            line_num,
            text: rest.to_string(),
            done: false,
            raw_line: line.to_string(),
        });
    }

    // Numbered: 1. item (handles multi-digit)
    if let Some((num_part, rest)) = trimmed.split_once(". ") {
        if num_part.chars().all(|c| c.is_ascii_digit()) && !num_part.is_empty() {
            return Some(Item {
                line_num,
                text: rest.to_string(),
                done: false,
                raw_line: line.to_string(),
            });
        }
    }

    None
}

/// Detect the predominant format in a list of items
fn detect_format(items: &[Item]) -> ItemFormat {
    if items.is_empty() {
        return ItemFormat::Bullet; // sensible default
    }

    let mut checkbox_count = 0;
    let mut numbered_count = 0;
    let mut bullet_count = 0;
    let mut asterisk_count = 0;

    for item in items {
        let trimmed = item.raw_line.trim();
        if trimmed.starts_with("- [") {
            checkbox_count += 1;
        } else if trimmed.starts_with("- ") {
            bullet_count += 1;
        } else if trimmed.starts_with("* ") {
            asterisk_count += 1;
        } else if trimmed
            .split_once('.')
            .map(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
        {
            numbered_count += 1;
        }
    }

    // Return the most common format
    let max = checkbox_count
        .max(numbered_count)
        .max(bullet_count)
        .max(asterisk_count);

    if max == 0 {
        ItemFormat::Bullet
    } else if checkbox_count == max {
        ItemFormat::Checkbox
    } else if numbered_count == max {
        ItemFormat::Numbered
    } else if asterisk_count == max {
        ItemFormat::Asterisk
    } else {
        ItemFormat::Bullet
    }
}

/// Find the primary section (the one to use for add/done operations)
fn find_primary_section(sections: &[Section], config_primary: Option<&str>) -> Option<usize> {
    // First, check config-specified primary section
    if let Some(primary) = config_primary {
        let primary_lower = primary.to_lowercase();
        for (i, section) in sections.iter().enumerate() {
            if section.name.to_lowercase().contains(&primary_lower)
                || section.path.to_lowercase().contains(&primary_lower)
            {
                return Some(i);
            }
        }
    }

    // Then, look for priority names
    for priority_name in PRIMARY_SECTION_NAMES {
        for (i, section) in sections.iter().enumerate() {
            if section.name.to_lowercase().contains(priority_name) {
                return Some(i);
            }
        }
    }

    // Fall back to first section with items, or just first section
    sections
        .iter()
        .position(|s| !s.items.is_empty())
        .or_else(|| if sections.is_empty() { None } else { Some(0) })
}

/// Format a new item in the given format
fn format_item(text: &str, format: ItemFormat, number: Option<usize>) -> String {
    match format {
        ItemFormat::Checkbox => format!("- [ ] {}", text),
        ItemFormat::Numbered => format!("{}. {}", number.unwrap_or(1), text),
        ItemFormat::Bullet => format!("- {}", text),
        ItemFormat::Asterisk => format!("* {}", text),
        ItemFormat::Plain => text.to_string(),
    }
}

/// Add an item to a section
fn add_item(
    content: &str,
    section_name: Option<&str>,
    item_text: &str,
    config_primary: Option<&str>,
) -> Result<String, String> {
    let sections = parse_todo(content);

    let section_idx = if let Some(name) = section_name {
        sections
            .iter()
            .position(|s| s.name.to_lowercase().contains(&name.to_lowercase()))
            .ok_or_else(|| format!("Section '{}' not found", name))?
    } else {
        find_primary_section(&sections, config_primary).ok_or("No sections found in TODO.md")?
    };

    let section = &sections[section_idx];
    let format = section.format;

    // Find insertion point (after last item in section, or after header)
    let insert_after = section
        .items
        .last()
        .map(|i| i.line_num)
        .unwrap_or(section.header_line);

    // Calculate next number if numbered
    let next_num = if format == ItemFormat::Numbered {
        Some(section.items.len() + 1)
    } else {
        None
    };

    let new_line = format_item(item_text, format, next_num);

    // Build new content
    let lines: Vec<&str> = content.lines().collect();
    let mut result = String::new();

    for (i, line) in lines.iter().enumerate() {
        result.push_str(line);
        result.push('\n');
        if i == insert_after {
            result.push_str(&new_line);
            result.push('\n');
        }
    }

    // Handle edge case: inserting at end of file
    if insert_after >= lines.len() {
        result.push_str(&new_line);
        result.push('\n');
    }

    Ok(result)
}

/// Find item by fuzzy text match
fn find_item_by_text<'a>(section: &'a Section, query: &str) -> Result<&'a Item, String> {
    let query_lower = query.to_lowercase();

    // Exact substring match first
    let matches: Vec<_> = section
        .items
        .iter()
        .filter(|i| i.text.to_lowercase().contains(&query_lower))
        .collect();

    match matches.len() {
        0 => Err(format!("No item matching '{}' found", query)),
        1 => Ok(matches[0]),
        _ => {
            // Multiple matches - show them
            let mut msg = format!("Multiple items match '{}'. Be more specific:\n", query);
            for (i, item) in matches.iter().enumerate() {
                msg.push_str(&format!("  {}. {}\n", i + 1, item.text));
            }
            Err(msg)
        }
    }
}

/// Mark an item as done (toggle checkbox or add [x])
fn mark_item_done(
    content: &str,
    query: &str,
    section_name: Option<&str>,
    config_primary: Option<&str>,
) -> Result<(String, String), String> {
    let sections = parse_todo(content);
    let section = if let Some(name) = section_name {
        find_section_by_name(&sections, name)?
    } else {
        let idx = find_primary_section(&sections, config_primary).ok_or("No sections found")?;
        &sections[idx]
    };

    let item = find_item_by_text(section, query)?;
    let lines: Vec<&str> = content.lines().collect();

    // Build new content with item marked as done
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == item.line_num {
            // Transform the line based on format
            let new_line = mark_line_done(line);
            result.push_str(&new_line);
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    Ok((result, item.text.clone()))
}

/// Transform a line to mark it as done
fn mark_line_done(line: &str) -> String {
    let trimmed = line.trim();

    // Already done
    if trimmed.starts_with("- [x]") || trimmed.starts_with("- [X]") {
        return line.to_string();
    }

    // Checkbox: - [ ] -> - [x]
    if trimmed.starts_with("- [ ] ") {
        return line.replace("- [ ] ", "- [x] ");
    }

    // Other formats: prepend [x]
    // For bullets: - item -> - [x] item
    if let Some(rest) = trimmed.strip_prefix("- ") {
        let indent = &line[..line.len() - line.trim_start().len()];
        return format!("{}- [x] {}", indent, rest);
    }

    // For numbered: 1. item -> 1. [x] item
    if let Some((num, rest)) = trimmed.split_once(". ") {
        if num.chars().all(|c| c.is_ascii_digit()) {
            let indent = &line[..line.len() - line.trim_start().len()];
            return format!("{}{}. [x] {}", indent, num, rest);
        }
    }

    // Fallback: just return as-is
    line.to_string()
}

/// Remove an item by text match
fn remove_item(
    content: &str,
    query: &str,
    section_name: Option<&str>,
    config_primary: Option<&str>,
) -> Result<(String, String), String> {
    let sections = parse_todo(content);
    let section = if let Some(name) = section_name {
        find_section_by_name(&sections, name)?
    } else {
        let idx = find_primary_section(&sections, config_primary).ok_or("No sections found")?;
        &sections[idx]
    };

    let item = find_item_by_text(section, query)?;
    let lines: Vec<&str> = content.lines().collect();

    // Build new content without the item line
    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i != item.line_num {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    // Renumber if using numbered format
    if section.format == ItemFormat::Numbered {
        result = renumber_section(&result, &section.name);
    }

    Ok((result, item.text.clone()))
}

/// Renumber items in a section
fn renumber_section(content: &str, section_name: &str) -> String {
    let mut in_section = false;
    let mut item_num = 1;
    let mut result = String::new();

    for line in content.lines() {
        if let Some((_, name)) = parse_header(line) {
            in_section = name.to_lowercase().contains(&section_name.to_lowercase());
            item_num = 1;
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_section {
            let trimmed = line.trim();
            // Check if it's a numbered item
            if let Some((num_str, rest)) = trimmed.split_once(". ") {
                if num_str.chars().all(|c| c.is_ascii_digit()) {
                    let indent = &line[..line.len() - line.trim_start().len()];
                    result.push_str(&format!("{}{}. {}\n", indent, item_num, rest));
                    item_num += 1;
                    continue;
                }
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline if content didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Find section by name or path (fuzzy match)
/// Supports:
/// - Simple name: "Backlog" (matches section with name containing "Backlog")
/// - Path: "Backlog/Language" (matches section with path containing both parts)
/// Returns error if multiple matches (ambiguous) or no matches
fn find_section_by_name<'a>(sections: &'a [Section], query: &str) -> Result<&'a Section, String> {
    let query_lower = query.to_lowercase();

    let matches: Vec<_> = if query.contains('/') {
        // Match against full path
        sections
            .iter()
            .filter(|s| s.path.to_lowercase().contains(&query_lower))
            .collect()
    } else {
        // Match against name only
        sections
            .iter()
            .filter(|s| s.name.to_lowercase().contains(&query_lower))
            .collect()
    };

    match matches.len() {
        0 => Err(format!("No section matching '{}' found", query)),
        1 => Ok(matches[0]),
        _ => {
            let mut msg = format!("Multiple sections match '{}'. Be more specific:\n", query);
            for section in matches {
                msg.push_str(&format!("  {}\n", section.path));
            }
            Err(msg)
        }
    }
}

/// Display sections with proper headers and filtering
fn display_sections(sections: &[Section], filter: &ListFilter, config: &TodoConfig, json: bool) {
    // Check if we should show all sections (from config or filter)
    let show_all = filter.all || config.show_all;

    // Determine which sections to show
    let sections_to_show: Vec<&Section> = if let Some(ref section_filter) = filter.section {
        let filter_lower = section_filter.to_lowercase();
        sections
            .iter()
            .filter(|s| {
                if section_filter.contains('/') {
                    // Path query - match against full path
                    s.path.to_lowercase().contains(&filter_lower)
                } else {
                    // Simple name query
                    s.name.to_lowercase().contains(&filter_lower)
                }
            })
            .collect()
    } else if show_all || filter.done {
        // Show all sections when filtering globally
        sections.iter().collect()
    } else {
        // Default: just primary section
        if let Some(idx) = find_primary_section(sections, config.primary_section.as_deref()) {
            vec![&sections[idx]]
        } else {
            vec![]
        }
    };

    // Filter items by status
    let status_filter = |item: &Item| -> bool {
        if filter.all {
            true
        } else if filter.done {
            item.done
        } else {
            // default: pending only
            !item.done
        }
    };

    if json {
        let sections_json: Vec<_> = sections_to_show
            .iter()
            .map(|s| {
                let items: Vec<_> = s
                    .items
                    .iter()
                    .filter(|i| status_filter(i))
                    .enumerate()
                    .map(|(i, item)| {
                        serde_json::json!({
                            "index": i + 1,
                            "text": item.text,
                            "done": item.done
                        })
                    })
                    .collect();
                serde_json::json!({
                    "name": s.name,
                    "path": s.path,
                    "level": s.header_level,
                    "format": format!("{:?}", s.format),
                    "items": items
                })
            })
            .filter(|s| !s["items"].as_array().unwrap().is_empty())
            .collect();
        println!("{}", serde_json::to_string_pretty(&sections_json).unwrap());
    } else {
        let mut any_output = false;

        for section in sections_to_show {
            let filtered_items: Vec<_> =
                section.items.iter().filter(|i| status_filter(i)).collect();

            if filtered_items.is_empty() {
                continue;
            }

            if any_output {
                println!(); // blank line between sections
            }

            // Print header with proper level
            println!("{} {}", "#".repeat(section.header_level), section.name);

            for item in &filtered_items {
                let marker = if item.done { "[x]" } else { "[ ]" };
                // Use the original format for display
                let prefix = match section.format {
                    ItemFormat::Checkbox => format!("- {} ", marker),
                    ItemFormat::Numbered => {
                        if item.done {
                            format!("- {} ", marker)
                        } else {
                            "- ".to_string()
                        }
                    }
                    ItemFormat::Bullet => {
                        if item.done {
                            format!("- {} ", marker)
                        } else {
                            "- ".to_string()
                        }
                    }
                    ItemFormat::Asterisk => {
                        if item.done {
                            format!("* {} ", marker)
                        } else {
                            "* ".to_string()
                        }
                    }
                    ItemFormat::Plain => {
                        if item.done {
                            format!("{} ", marker)
                        } else {
                            "".to_string()
                        }
                    }
                };
                println!("{}{}", prefix, item.text);
            }

            any_output = true;
        }

        if !any_output {
            if filter.done {
                println!("No completed items");
            } else {
                println!("No pending items");
            }
        }
    }
}

/// Remove all completed items from the file
fn clean_done_items(content: &str) -> String {
    let sections = parse_todo(content);
    let done_lines: std::collections::HashSet<usize> = sections
        .iter()
        .flat_map(|s| s.items.iter().filter(|i| i.done).map(|i| i.line_num))
        .collect();

    let mut result = String::new();
    for (i, line) in content.lines().enumerate() {
        if !done_lines.contains(&i) {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Preserve trailing newline behavior
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    result
}

/// Common todo file names to auto-detect (in priority order)
const TODO_FILE_NAMES: &[&str] = &[
    "TODO.md",
    "TODO.txt",
    "TODO",
    "TASKS.md",
    "TASKS.txt",
    "TASKS",
    "todo.md",
    "todo.txt",
    "todo",
    "tasks.md",
    "tasks.txt",
    "tasks",
];

/// Find a todo file in the given directory
fn find_todo_file(root: &Path) -> Option<std::path::PathBuf> {
    for name in TODO_FILE_NAMES {
        let path = root.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Main command handler
pub fn cmd_todo(action: Option<TodoAction>, file: Option<&Path>, json: bool, root: &Path) -> i32 {
    // Load config
    let config = MossConfig::load(root);
    let todo_config = &config.todo;

    // Determine the todo file path
    // Priority: --file flag > config.todo.file > auto-detect
    let todo_path = if let Some(f) = file {
        if f.is_absolute() {
            f.to_path_buf()
        } else {
            root.join(f)
        }
    } else if let Some(ref config_file) = todo_config.file {
        root.join(config_file)
    } else {
        match find_todo_file(root) {
            Some(p) => p,
            None => {
                eprintln!(
                    "No todo file found in {}. Looked for: {}",
                    root.display(),
                    TODO_FILE_NAMES.join(", ")
                );
                return 1;
            }
        }
    };

    if !todo_path.exists() {
        eprintln!("Todo file not found: {}", todo_path.display());
        return 1;
    }

    let content = match fs::read_to_string(&todo_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading {}: {}", todo_path.display(), e);
            return 1;
        }
    };

    let primary = todo_config.primary_section.as_deref();

    match action {
        Some(TodoAction::Add { text, section }) => {
            match add_item(&content, section.as_deref(), &text, primary) {
                Ok(new_content) => {
                    if let Err(e) = fs::write(&todo_path, &new_content) {
                        eprintln!("Error writing TODO.md: {}", e);
                        return 1;
                    }
                    if json {
                        println!("{}", serde_json::json!({"status": "added", "item": text}));
                    } else {
                        let sections = parse_todo(&content);
                        let section_name = if let Some(ref s) = section {
                            find_section_by_name(&sections, s)
                                .map(|sec| sec.name.as_str())
                                .unwrap_or(s.as_str())
                        } else {
                            find_primary_section(&sections, todo_config.primary_section.as_deref())
                                .map(|i| sections[i].name.as_str())
                                .unwrap_or("TODO")
                        };
                        println!("Added to {}: {}", section_name, text);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    1
                }
            }
        }

        Some(TodoAction::Done { query, section }) => {
            match mark_item_done(&content, &query, section.as_deref(), primary) {
                Ok((new_content, completed_item)) => {
                    if let Err(e) = fs::write(&todo_path, &new_content) {
                        eprintln!("Error writing TODO.md: {}", e);
                        return 1;
                    }
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"status": "completed", "item": completed_item})
                        );
                    } else {
                        println!("Marked done: {}", completed_item);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    1
                }
            }
        }

        Some(TodoAction::Rm { query, section }) => {
            match remove_item(&content, &query, section.as_deref(), primary) {
                Ok((new_content, removed_item)) => {
                    if let Err(e) = fs::write(&todo_path, &new_content) {
                        eprintln!("Error writing TODO.md: {}", e);
                        return 1;
                    }
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({"status": "removed", "item": removed_item})
                        );
                    } else {
                        println!("Removed: {}", removed_item);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    1
                }
            }
        }

        Some(TodoAction::Clean) => {
            let sections = parse_todo(&content);
            let done_count: usize = sections
                .iter()
                .map(|s| s.items.iter().filter(|i| i.done).count())
                .sum();

            if done_count == 0 {
                if json {
                    println!("{}", serde_json::json!({"status": "clean", "removed": 0}));
                } else {
                    println!("No completed items to remove");
                }
                return 0;
            }

            let new_content = clean_done_items(&content);
            if let Err(e) = fs::write(&todo_path, &new_content) {
                eprintln!("Error writing TODO.md: {}", e);
                return 1;
            }

            if json {
                println!(
                    "{}",
                    serde_json::json!({"status": "clean", "removed": done_count})
                );
            } else {
                println!("Removed {} completed item(s)", done_count);
            }
            0
        }

        None => {
            let sections = parse_todo(&content);
            display_sections(&sections, &ListFilter::default(), todo_config, json);
            0
        }

        Some(TodoAction::List { filter }) => {
            if filter.raw {
                if json {
                    println!("{}", serde_json::json!({"content": content}));
                } else {
                    print!("{}", content);
                }
                return 0;
            }

            let sections = parse_todo(&content);
            display_sections(&sections, &filter, todo_config, json);
            0
        }
    }
}
