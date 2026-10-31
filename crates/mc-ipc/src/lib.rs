//! JSON-RPC 2.0 wire types shared by the manager, frontends and plugins.
//!
//! Framing: one JSON object per line (`\n` terminated). The transport is
//! normally stdio, but any byte stream works (unix socket, TCP, pipe).

use mc_core::{CrashInfo, LogLine, ProcessState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const JSONRPC: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC.to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC.to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC.to_string(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// Server-pushed notifications. Long running operations answer immediately
/// with a handle and then stream these until they finish.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Event {
    #[serde(rename = "log.line")]
    LogLine { session: String, line: LogLine },

    #[serde(rename = "proc.state")]
    ProcessState {
        session: String,
        state: ProcessState,
    },

    #[serde(rename = "proc.exit")]
    ProcessExit {
        session: String,
        code: Option<i32>,
        crash: Option<CrashInfo>,
    },

    #[serde(rename = "download.progress")]
    DownloadProgress {
        label: String,
        done: u64,
        total: u64,
    },
}

impl Event {
    pub fn session(&self) -> &str {
        match self {
            Event::LogLine { session, .. }
            | Event::ProcessState { session, .. }
            | Event::ProcessExit { session, .. } => session,
            Event::DownloadProgress { .. } => "",
        }
    }
}

/// Well-known request methods.
pub mod method {
    pub const VERSION_LIST: &str = "version.list";
    pub const VERSION_INSTALL: &str = "version.install";
    pub const INSTANCE_LIST: &str = "instance.list";
    pub const INSTANCE_LAUNCH: &str = "instance.launch";
    pub const INSTANCE_ATTACH: &str = "instance.attach";
    pub const INSTANCE_STOP: &str = "instance.stop";
    pub const INSTANCE_KILL: &str = "instance.kill";
    pub const INSTANCE_SEND: &str = "instance.send";
    pub const AUTH_LOGIN: &str = "auth.login";
    pub const AUTH_STATUS: &str = "auth.status";
    pub const SYSTEM_INFO: &str = "system.info";
    pub const SHUTDOWN: &str = "system.shutdown";
}

pub fn encode<T: Serialize>(msg: &T) -> Result<String, serde_json::Error> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    Ok(s)
}

pub fn decode_request(line: &str) -> Result<Request, serde_json::Error> {
    serde_json::from_str(line)
}

pub fn decode_response(line: &str) -> Result<Response, serde_json::Error> {
    serde_json::from_str(line)
}

pub fn decode_event(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str(line)
}
