// Claude Code CLI WebSocket Protocol Module
//
// This module implements direct communication with Claude Code CLI
// using the WebSocket SDK protocol (--sdk-url flag) or stdin/stdout.

pub mod cli_args;
pub mod cli_settings;
pub mod client;
pub mod factory;
pub mod message_normalizer;
pub mod permission_rules;
pub mod process;
pub mod protocol;
pub mod provider_bridge;
pub mod server;
pub mod session;
pub mod session_file;
pub mod session_manager_trait;
pub mod session_registry;
pub mod session_wrapper;
pub mod stdin_session;
pub mod transport;
pub mod transports;

// Re-exports for potential external use
#[allow(unused)]
pub use protocol::*;
#[allow(unused)]
pub use session::ClaudeSessionManager;
#[allow(unused)]
pub use session_file::SessionFileReader;
#[allow(unused)]
pub use session_registry::{get_session_registry, SessionRegistry};
#[allow(unused)]
pub use session_wrapper::SessionManagerWrapper;
#[allow(unused)]
pub use stdin_session::StdinSessionManager;
#[allow(unused)]
pub use transport::{ClaudeCliProcess, ClaudeTransport, ProcessError, TransportError};
