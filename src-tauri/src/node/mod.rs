// Node.js backend integration module

pub mod process;
pub mod rpc;

// Legacy exports - kept for potential future use
#[allow(unused)]
pub use process::{NodeProcess, GLOBAL_NODE_PROCESS};
#[allow(unused)]
pub use rpc::{NotificationHandler, RpcClient, GLOBAL_RPC_CLIENT};
