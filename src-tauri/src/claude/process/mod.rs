// Process Management Module
//
// This module contains implementations of the ClaudeCliProcess trait
// for managing the Claude Code CLI process lifecycle.

pub mod cli_process;
pub mod stdin_process;

pub use cli_process::CliProcessManager;
pub use stdin_process::StdinProcessManager;
