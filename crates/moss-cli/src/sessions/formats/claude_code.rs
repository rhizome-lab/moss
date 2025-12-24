//! Claude Code JSONL format parser.

use super::{peek_lines, LogFormat};
use crate::sessions::{
    categorize_error, normalize_path, ErrorPattern, SessionAnalysis, TokenStats, ToolStats,
};
use serde_json::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Claude Code session log format (JSONL).
pub struct ClaudeCodeFormat;

impl LogFormat for ClaudeCodeFormat {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn detect(&self, path: &Path) -> f64 {
        // Check extension
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "jsonl" {
            return 0.0;
        }

        // Peek at first few lines
        for line in peek_lines(path, 5) {
            if let Ok(entry) = serde_json::from_str::<Value>(&line) {
                // Claude Code has type field with specific values
                if let Some(t) = entry.get("type").and_then(|v| v.as_str()) {
                    if matches!(t, "user" | "assistant" | "summary" | "file-history-snapshot") {
                        return 1.0;
                    }
                }
            }
        }
        0.0
    }

    fn analyze(&self, path: &Path) -> Result<SessionAnalysis, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let reader = BufReader::new(file);

        let mut analysis = SessionAnalysis::new(path.to_path_buf(), self.name());
        let mut entries: Vec<Value> = Vec::new();

        // Parse all JSONL entries
        for line in reader.lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<Value>(&line) {
                entries.push(entry);
            }
        }

        // Count message types
        for entry in &entries {
            if let Some(msg_type) = entry.get("type").and_then(|v| v.as_str()) {
                *analysis.message_counts.entry(msg_type.to_string()).or_insert(0) += 1;
            }
        }

        // Analyze tools
        analysis.tool_stats = analyze_tools(&entries);

        // Analyze tokens
        analysis.token_stats = analyze_tokens(&entries);

        // Find error patterns
        analysis.error_patterns = find_error_patterns(&entries);

        // Analyze file token usage
        analysis.file_tokens = analyze_file_tokens(&entries);

        // Count turns and parallel opportunities
        analysis.total_turns = count_turns(&entries);
        analysis.parallel_opportunities = find_parallel_opportunities(&entries);

        Ok(analysis)
    }
}

fn analyze_tools(entries: &[Value]) -> HashMap<String, ToolStats> {
    let mut stats: HashMap<String, ToolStats> = HashMap::new();

    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }

        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        for block in content {
            if block.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                if let Some(tool_name) = block.get("name").and_then(|v| v.as_str()) {
                    let stat = stats
                        .entry(tool_name.to_string())
                        .or_insert_with(|| ToolStats::new(tool_name));
                    stat.calls += 1;
                }
            }
        }
    }

    // Count errors from tool results in user messages
    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }

        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        for block in content {
            if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                if block.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                    // Tool errors are hard to attribute without tracking tool_use_id
                    // For now, we count them in error patterns
                }
            }
        }
    }

    stats
}

fn analyze_tokens(entries: &[Value]) -> TokenStats {
    let mut stats = TokenStats::default();
    let mut request_data: HashMap<String, TokenData> = HashMap::new();

    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }

        let Some(usage) = entry.get("message").and_then(|m| m.get("usage")) else {
            continue;
        };

        let request_id = entry
            .get("requestId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_create = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        // Take max values per request (streaming updates)
        let data = request_data.entry(request_id).or_default();
        data.input = data.input.max(input);
        data.output = data.output.max(output);
        data.cache_read = data.cache_read.max(cache_read);
        data.cache_create = data.cache_create.max(cache_create);
    }

    // Aggregate
    for data in request_data.values() {
        if data.input > 0 || data.cache_read > 0 {
            stats.api_calls += 1;
            stats.total_input += data.input;
            stats.total_output += data.output;
            stats.cache_read += data.cache_read;
            stats.cache_create += data.cache_create;

            let context_size = data.input + data.cache_read;
            stats.update_context(context_size);
        }
    }

    stats
}

#[derive(Default)]
struct TokenData {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_create: u64,
}

fn find_error_patterns(entries: &[Value]) -> Vec<ErrorPattern> {
    let mut categories: HashMap<&str, Vec<String>> = HashMap::new();

    for entry in entries {
        // Check user messages for tool_result with is_error
        if entry.get("type").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }

        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        for block in content {
            if block.get("type").and_then(|v| v.as_str()) == Some("tool_result")
                && block.get("is_error").and_then(|v| v.as_bool()) == Some(true)
            {
                let error_text = block
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect::<String>();

                let category = categorize_error(&error_text);
                categories
                    .entry(category)
                    .or_default()
                    .push(error_text);
            }
        }
    }

    let mut patterns: Vec<ErrorPattern> = categories
        .into_iter()
        .map(|(category, examples)| {
            let mut pattern = ErrorPattern::new(category);
            pattern.count = examples.len();
            pattern.examples = examples.into_iter().take(3).collect();
            pattern
        })
        .collect();

    patterns.sort_by(|a, b| b.count.cmp(&a.count));
    patterns
}

fn analyze_file_tokens(entries: &[Value]) -> HashMap<String, u64> {
    let mut file_tokens: HashMap<String, u64> = HashMap::new();
    let mut requests: HashMap<String, RequestData> = HashMap::new();

    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }

        let request_id = entry
            .get("requestId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let output_tokens = entry
            .get("message")
            .and_then(|m| m.get("usage"))
            .and_then(|u| u.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let data = requests.entry(request_id).or_default();
        data.output_tokens = data.output_tokens.max(output_tokens);

        // Extract file paths from tool calls
        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        for block in content {
            if block.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                continue;
            }

            let tool_name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let input = block.get("input");

            // Extract file_path from Read, Edit, Write
            if let Some(fp) = input.and_then(|i| i.get("file_path")).and_then(|v| v.as_str()) {
                data.paths.push(fp.to_string());
            }

            // Extract path from Grep
            if let Some(p) = input.and_then(|i| i.get("path")).and_then(|v| v.as_str()) {
                data.paths.push(p.to_string());
            }

            // Extract from Bash commands (moss view/analyze)
            if tool_name == "Bash" {
                if let Some(cmd) = input.and_then(|i| i.get("command")).and_then(|v| v.as_str()) {
                    data.paths.extend(extract_symbol_paths_from_bash(cmd));
                }
            }

            // Extract directory from Glob pattern
            if tool_name == "Glob" {
                if let Some(pattern) = input.and_then(|i| i.get("pattern")).and_then(|v| v.as_str())
                {
                    if let Some(dir) = pattern.rsplit_once('/') {
                        if !dir.0.starts_with('*') {
                            data.paths.push(dir.0.to_string());
                        }
                    }
                }
            }
        }
    }

    // Distribute tokens to paths
    for data in requests.values() {
        if data.paths.is_empty() || data.output_tokens == 0 {
            continue;
        }
        let per_path = data.output_tokens / data.paths.len() as u64;
        for path in &data.paths {
            let norm = normalize_path(path);
            *file_tokens.entry(norm).or_insert(0) += per_path;
        }
    }

    file_tokens
}

#[derive(Default)]
struct RequestData {
    output_tokens: u64,
    paths: Vec<String>,
}

fn extract_symbol_paths_from_bash(command: &str) -> Vec<String> {
    let mut paths = Vec::new();

    // Match: moss view <path> or uv run moss view <path>
    let re = regex::Regex::new(r"(?:uv run )?moss (?:view|analyze)\s+([^\s]+)").unwrap();
    for cap in re.captures_iter(command) {
        let path = &cap[1];
        if path.starts_with('-') {
            continue;
        }
        // Check if it looks like a symbol path (has / after file extension)
        if regex::Regex::new(r"\.\w+/\w").unwrap().is_match(path) {
            paths.push(path.to_string());
        }
    }

    paths
}

fn count_turns(entries: &[Value]) -> usize {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) == Some("assistant") {
            let request_id = entry
                .get("requestId")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            seen.insert(request_id);
        }
    }
    seen.len()
}

fn find_parallel_opportunities(entries: &[Value]) -> usize {
    let mut single_tool_turns = 0;

    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) != Some("assistant") {
            continue;
        }

        let Some(content) = entry
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_array())
        else {
            continue;
        };

        let tool_uses: Vec<_> = content
            .iter()
            .filter(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
            .collect();

        if tool_uses.len() == 1 {
            single_tool_turns += 1;
        }
    }

    single_tool_turns
}
