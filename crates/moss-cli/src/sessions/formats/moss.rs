//! Moss internal session JSON format parser.

use super::{read_file, LogFormat};
use crate::sessions::{SessionAnalysis, TokenStats, ToolStats};
use serde_json::Value;
use std::path::Path;

/// Moss internal session format (JSON).
pub struct MossFormat;

impl LogFormat for MossFormat {
    fn name(&self) -> &'static str {
        "moss"
    }

    fn detect(&self, path: &Path) -> f64 {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ext != "json" {
            return 0.0;
        }

        let Ok(content) = read_file(path) else {
            return 0.0;
        };

        let Ok(data) = serde_json::from_str::<Value>(&content) else {
            return 0.0;
        };

        // Moss sessions have tool_calls and llm_calls fields
        if data.get("tool_calls").is_some() && data.get("llm_calls").is_some() {
            return 1.0;
        }

        // Also check for workspace and task (Session struct)
        if data.get("workspace").is_some() && data.get("task").is_some() {
            return 0.9;
        }

        0.0
    }

    fn analyze(&self, path: &Path) -> Result<SessionAnalysis, String> {
        let content = read_file(path)?;
        let data: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

        let mut analysis = SessionAnalysis::new(path.to_path_buf(), self.name());

        // Extract tool stats
        if let Some(tool_calls) = data.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tool_calls {
                let tool_name = tc
                    .get("tool_name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");

                let stat = analysis
                    .tool_stats
                    .entry(tool_name.to_string())
                    .or_insert_with(|| ToolStats::new(tool_name));
                stat.calls += 1;

                if tc.get("error").is_some() {
                    stat.errors += 1;
                }
            }
        }

        // Token stats
        analysis.token_stats = TokenStats {
            total_input: data
                .get("llm_tokens_in")
                .or_else(|| data.get("tokens_in"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_output: data
                .get("llm_tokens_out")
                .or_else(|| data.get("tokens_out"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            api_calls: data
                .get("llm_calls")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            ..Default::default()
        };

        // Turns = LLM calls
        analysis.total_turns = analysis.token_stats.api_calls;

        // Message counts from status
        if let Some(status) = data.get("status").and_then(|s| s.as_str()) {
            analysis.message_counts.insert(status.to_string(), 1);
        }

        Ok(analysis)
    }
}
