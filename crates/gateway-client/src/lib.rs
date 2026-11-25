//! A client for chat-completions servers, plus the few opt-in extras that the
//! Runvil Gateway adds.
//!
//! [`GatewayChat`] is the HTTP client. It sends one round to one server URL
//! and presents the server's shared bearer key when built with one. Every
//! request streams: the server's `/chat/completions` endpoint answers with
//! server-sent events (SSE). [`GatewayChat::complete`] sends the request body
//! and reads the stream within the client's byte cap and per-receive timeout.
//! It calls the caller's callback with each [`StreamDelta`] as it arrives and
//! returns the one [`Reply`] the round produced. [`fetch_models`] lists the
//! server's models.
//!
//! [`Gateway`] says where a Gateway is and how to log in. A program that knows
//! the address builds one with [`Gateway::new`]. A program configured through
//! the environment calls [`Gateway::find_env`]. With the `discovery` feature,
//! which is on by default, `Gateway::find` reads the file that a Gateway on
//! this machine writes. [`Gateway::client`] makes the [`GatewayChat`] for it.
//!
//! A round takes its messages as any type that serde writes as a
//! chat-completions message, so a caller with its own message type passes it
//! as it is. [`Message`] is for a caller that has none. [`Tool`] and
//! [`Options`] describe the tools and the request settings.
//!
//! ## Invariants
//!
//! - Every failure is an [`Error`] with an [`ErrorKind`]. Its `summary` holds
//!   this crate's own wording and never server text. Server text goes in the
//!   `message` only, bounded and control-escaped.
//! - A bearer key is never written to logs, `Debug`, `Display`, or error text.
//! - The key leaves a [`SecretString`] in two places only: the `Authorization`
//!   header of a request, and [`SecretString::expose_secret`], which a caller
//!   uses to write the key where a program that calls the server itself reads
//!   it.
//! - A client sends requests without a key only when the caller builds it with
//!   `GatewayChat::keyless`, which does not check the endpoint's address, or
//!   when [`Gateway::find_env`] finds no key for a loopback URL. A client
//!   built with `GatewayChat::disabled` holds no key and sends no requests.
//! - The reply of a round has passed one set of rules: the byte cap, the
//!   `[DONE]` sentinel, the truncation rule for a batch of tool calls, and the
//!   strict turn parse.

mod config;
#[cfg(feature = "discovery")]
mod discovery;
mod error;
mod failure;
mod gateway;
mod message;
mod models;
mod options;
mod reply;
mod transport;
mod wire;

pub use config::{GatewayEndpoint, SecretString};
pub use error::{Error, ErrorKind};
pub use gateway::Gateway;
pub use message::{Message, Role};
pub use models::{Model, Thinking, fetch_models};
pub use options::{Options, Tool};
pub use reply::{ClientTiming, Outcome, Reply, ToolCall, Usage};
pub use transport::GatewayChat;
pub use wire::delta::StreamDelta;
pub use wire::stream::escape_controls;
