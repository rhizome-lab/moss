//! Workflow engine with Lua scripting and LLM support.
//!
//! # Why Lua?
//!
//! We evaluated several options for workflow definitions:
//!
//! - **TOML**: Too rigid. Once you need `if is_dirty() then commit() end`, you're
//!   fighting the format. We tried this first and deleted ~1500 lines.
//! - **Shell scripts**: Awkward composition, no structured data return values.
//! - **Custom DSL**: More implementation work, yet another language to learn.
//! - **Rhai**: Rust-native but smaller ecosystem than Lua.
//!
//! Lua hits the sweet spot:
//! - LuaJIT is ~200KB, extremely fast, minimal overhead
//! - Syntax is almost as simple as TOML: `view("foo.rs")` vs `view: foo.rs`
//! - But you get loops, conditionals, variables, functions when needed
//! - Industry-proven for scripting (games, nginx, redis, neovim)
//!
//! The key insight: the boundary between "config" and "script" is fuzzy.
//! Once you need conditionals, you're writing code. Might as well use a real
//! (but minimal) language.

#[cfg(feature = "lua")]
mod lua_runtime;

#[cfg(feature = "llm")]
pub(crate) mod llm;

mod memory;
mod shadow;

#[cfg(feature = "lua")]
pub use lua_runtime::{CommandResult, LuaRuntime, RuntimeState, RuntimeYield, WorkflowSession};

pub use shadow::{Hunk, ShadowGit, SnapshotId};
