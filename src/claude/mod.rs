// Claude Code CLI WebSocket Protocol Module
//
// This module implements direct communication with Claude Code CLI
// using the WebSocket SDK protocol (--sdk-url flag).

// Note: Source files are located at the project root,
// not in a src-tauri/src/ subdirectory structure.

pub mod protocol;
pub mod server;
pub mod client;
pub mod session;
pub mod session_file;

// Re-exports for potential external use
#[allow(unused)]
pub use session::ClaudeSessionManager;
#[allow(unused)]
pub use session_file::SessionFileReader;
#[allow(unused)]
pub use protocol::*;
