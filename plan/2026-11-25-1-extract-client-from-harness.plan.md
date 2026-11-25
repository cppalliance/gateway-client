---
name: Build gateway-client
overview: Build the independent gateway-client crate in this repository. It has its own plain types, takes messages through serde, and uses no non_exhaustive. Pull request 1 holds the types, the port of the wire code, and the first CI. Pull request 2 adds the discovery reader and the Gateway type. The Harness adapter, Workshop, and the other repositories are in the parent plan and are not described here.
todos:
  - id: client-scaffold-push
    content: "A1: scaffold copied from runvil-harness, first commit pushed straight to master (a pull request needs a base branch, and the remote was empty)"
    status: pending
  - id: client-types
    content: "A2: write error.rs, failure.rs, message.rs, options.rs, reply.rs"
    status: pending
  - id: client-port
    content: "A3: copy the wire, transport, config, and models code and their tests from harness-gateway-client, applying only the edits listed in Part 4"
    status: pending
  - id: client-pr1
    content: "A4 to A6: first compile and fixes, snapshot, README; pull request 1, CI, fast-forward merge"
    status: pending
  - id: client-pr2
    content: "Stage C: discovery reader, the Gateway type, and the fixture check; pull request 2, CI, fast-forward merge"
    status: pending
isProject: false
---

# Build gateway-client

This is the part of the extraction plan that concerns this repository. The parent plan also covers the Harness adapter, Workshop, `runvil`, `runvil-gateway`, and `runvil-design`.

## Decisions this plan uses

- **One new crate, `gateway-client`, in the repository `cppalliance/gateway-client`.** It depends on no Runvil crate. The repository name is settled. The crate name is still a proposal, and a rename is cheap while pins are by Git commit.
- **The Engine and the `runvil` facade do not change.** Nothing in the Engine depends on an outside repository, and `cargo xtask api` sees no new re-export.
- **Messages cross by serde.** The client's request function takes `&[M] where M: Serialize + Sync`. The Engine's `Message` already derives `Serialize` in the OpenAI shape, so the adapter passes `&messages` straight in. `InferenceBroker::chat()` keeps its signature.
- **Tools and options are copied through the Engine's public getters.** The client has its own `Tool` and `Options`.
- **The client returns its own `Reply`. The adapter turns it into the Engine's `Completion`** through the Engine's constructors, so the Engine's checks stay in the Engine.
- **The client has its own `Error` with a `kind`.** The adapter maps it to the Engine's `Error` and `ErrorCode`.
- **No `#[non_exhaustive]` in the client, on enums, on `Error`, or on structs.** A wildcard arm in another crate turns a new variant into a run-time case, and the adapter that matches on these enums is pinned and built with the client, so a compile error on change is what we want. It also removes `ErrorCode::Internal` from the mapping and lets the adapter's tests build a client `Error` directly. On a struct with private fields the attribute did nothing. The cost is that adding a variant or a public field is a breaking change for anyone pinned to the crate, and they choose when to take it.
- **`Message` has one public text constructor.** `Message::text(role, content)` and `Message::tool(tool_call_id, content)`, with `Role` public (19 public names).
- **`Tool::new` takes the parameter schema as a JSON object (`Map<String, Value>`), not any `Value`.** The data is a JSON Schema object that tool authors write as JSON (a Plugin hands it across the C contract as text, and Lua's `tools.adhoc` converts a table), so a Rust model of JSON Schema would drop keywords or reimplement the standard. The type only guarantees what the Engine already checks, that it is an object. Confidence: medium.
- **Conversions in the adapter are traits of the adapter's own** (`IntoEngine`, `TryIntoEngine`, `IntoWire`, `TryIntoWire`), not `From` and `Into`. The orphan rule forbids `impl From<A> for B` in a crate that owns neither type, and the client's types and the Engine's types are both foreign to the adapter.
- **`harness-gateway-client` keeps its name and stays in `runvil-harness` as the adapter.** Workshop, the Harness tests, the Harness's product-boundary rules, and Workshop's `harness_pin` check all name that crate, so none of them change. The adapter re-exports `GatewayEndpoint` and `SecretString` from the client, because Workshop names them. `StreamDelta` stays inside the two crates: since the streaming change, the broker maps it to the Harness's `Delta` and no Host names it. The student programs in `runvil-classroom` pin an old Harness commit and keep building on it, and that repository is not part of this run. Optional later rename to `harness-gateway`: cosmetic, about 30 files. Confidence: medium.
- **Discovery is a feature of the client, on by default, reader only.** The Gateway repository keeps the writer, `GatewayInstanceLease`, and the six-field `gateway.json` contract. Workshop keeps the launch lock and shutdown. The reader never deletes the file.
- **Calls are explicit.** `Gateway::find()` reads the discovery file. `Gateway::find_env()` and `Gateway::find_env_named(url_var, key_var)` read environment variables. A known address is `Gateway::new(endpoint)`, plus `with_key(key)` when it needs one. Nothing reads the environment unless the Host asks. Confidence: high.
- **The image-name check moves unchanged.** The reader still requires the process image `runvil-gateway`.
- **Pinned by Git commit, not on crates.io.**

## What stays out

- The `cheapseek` repository: no edits, no writer crate, no contract change, no run-directory override.
- Launching a Gateway, the capability route, vendor-native protocol code, crates.io, and the opaque field's implementation.
- Renames in the client's public API (`Client`, `Chat`, builder style from the report's Appendix A). Stage A and B change where the code lives and which types it uses. They do not redesign the API.
- The transport trait from the report's section 6. `transport.rs` keeps calling `reqwest` and `tokio` directly, as today, and the trait can be added later without touching the rest of this plan.
- A `links` key on the client. Nothing shared crosses repositories yet (Part 10 says when to add it).

## Part 1. The new crate: layout, manifest, public names

### Layout

```text
crates/gateway-client/
  Cargo.toml
  src/
    lib.rs         NEW     the public names
    error.rs       NEW     Error, ErrorKind
    failure.rs     CHANGED turns reqwest and tokio errors into Error
    message.rs     NEW     Message
    options.rs     NEW     Tool, Options
    reply.rs       NEW     Reply, Outcome, ToolCall, Usage, ClientTiming
    config.rs      MOVED   GatewayEndpoint, SecretString (errors renamed, expose_secret is public)
    models.rs      MOVED   from catalog.rs; returns Vec<Model>, builds no Engine type
    transport.rs   CHANGED GatewayChat::complete is generic over the messages and returns Reply
    wire.rs        MOVED
    wire/classify.rs  MOVED  the status and body classifier
    wire/delta.rs     MOVED  `#[non_exhaustive]` removed from StreamDelta; nothing else
    wire/parse.rs     CHANGED metrics parsing for llama.cpp and vLLM leaves
    wire/read.rs      MOVED  returns Reply
    wire/request.rs   CHANGED generic over the messages, returns Result
    wire/stream.rs    CHANGED finish() builds a Reply
    (each *-tests.rs file moves next to the file it tests)
```

Files that do not move to the client: `broker.rs`, `timer.rs`, `resources.rs` and their tests. They become the adapter (Part 5).

### Manifest (new repository root)

The versions are the Harness workspace's own (Cargo.toml), so nothing new enters the lockfile.

```toml
# Cargo.toml (repository root)
[workspace]
resolver = "3"
members = ["crates/gateway-client"]

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.88"
license = "BSL-1.0"
repository = "https://github.com/cppalliance/gateway-client"

[workspace.dependencies]
bytes = "1"
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["float_roundtrip"] }
reqwest = { version = "0.13", default-features = false, features = ["json", "query", "rustls", "stream", "system-proxy"] }
url = "2"
tokio = { version = "1", features = ["time"] }
axum = "0.8"

# [workspace.lints.rust], [workspace.lints.rustdoc], [workspace.lints.clippy]: copy from
# runvil-harness/Cargo.toml unchanged, so every #[expect(clippy::...)] in the moved code still holds.
# [workspace.metadata.dylint]: copy it too, with the same rev of cppalliance/dylints.
```

```toml
# crates/gateway-client/Cargo.toml
[package]
name = "gateway-client"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
publish = false

[dependencies]
bytes.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
url.workspace = true

[features]
# Stage C turns this on by default and fills it. Until then it is empty.
default = []
discovery = []

[dev-dependencies]
axum.workspace = true
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "net", "io-util", "test-util"] }

[lints]
workspace = true
```

Notes for review:
- `runvil` and `runvil-engine` are gone from both `[dependencies]` and `[dev-dependencies]`. That is the whole point of the move.
- `rust-version = "1.88"` is a proposal. The code uses let chains (`if let Some(len) = ... && len > cap`), which need 1.88, and the Harness sets 1.95 only for the Engine's Lua crate. CI must build at 1.88 to prove it. Confidence: medium.
- The Harness pins `serde_json` with `float_roundtrip` because run logs store timings as JSON text. The client keeps the same feature so a timing parsed from a reply equals the one the adapter records.

### Repository scaffold (copied from the root of `runvil-harness`)

The checkout is empty, so the scaffold is part of the work. Every file below comes from the Harness, so the new repo builds, lints, and gates the way the other Runvil repositories do.

- `rust-toolchain.toml`: `channel = "1.99"` with `rustfmt` and `clippy`, as in the Harness. The crate's own floor is the `rust-version` in the manifest above.
- `rustfmt.toml`: `style_edition = "2024"`.
- `clippy.toml`: copied unchanged (test allowances, the 100-line and 7-argument limits, and the `disallowed-methods` list of process-global installers).
- `.cargo/config.toml`: the Windows block (`rust-lld` and the static CRT) and `CLIPPY_DISABLE_DOCS_LINKS = "1"`. The `xtask` and `outline` aliases stay behind, because the new repo has no xtask crate and no vibelight checkout.
- `.gitignore` (`/target`, `*.env`) and `.gitattributes` (`* text=auto eol=lf`).
- `LICENSE`: the same BSL-1.0 text.
- `deny.toml`: copied. The header comment names this repo. `[[bans.features]]` keeps `serde_json` without `preserve_order`, because the client uses `float_roundtrip` for the same reason. `allow-git` becomes empty, since the client has no git dependency.
- `Cargo.lock`, committed, because CI runs with `--locked`.
- `AGENTS.md`: short. What the crate is, and the one definition it needs: the Gateway is Runvil's server, and the client works against any chat-completions server. The principles it reuses: design for human review, no backward compatibility, never bump versions. Its own rules: no `#[non_exhaustive]`, the crate's own `Error` with `ErrorKind`, `unsafe` only in `discovery/sys`, `serde_json` with `float_roundtrip`. The verification commands: `cargo nextest run --locked --workspace --all-features`, clippy with `CARGO_BUILD_WARNINGS=deny`, `cargo fmt --all --check`, and `cargo doc --workspace --no-deps --all-features` with `RUSTDOCFLAGS="-D warnings"`.
- `README.md`: what it is, a ten-line example, and the `git` plus `rev` dependency line.
- `.github/workflows/ci.yml`: the Harness's jobs, minus the two that need Harness-only tools. Kept: `fmt`, `clippy`, `test` with the clean-tree step, `test-windows`, `docs`, `supply-chain` (cargo-deny, cargo-audit, and the "one rustls crypto backend" step, which matters here because this crate is where `reqwest` is), `dylint`, `msrv`, and `ci-green`. The `msrv` job reads `rust-version` from the root manifest and builds `-p gateway-client`. Left out: `wording` (needs `tools/docs-claims.mjs`) and `api-surface` (added with the snapshot, step A6 in Part 7). `ci-green`'s `needs` list must name every job.
- Not copied: `tools/`, `.githooks`, `.vibelight`, `vibelight.toml`, `vibe/`, `prompts/`, `decisions/`, `crates/build-xtask`.
- `plan/2026-11-25-1-extract-client-from-harness.plan.md`: this plan, committed in the repo's own convention (`plan/YYYY-MM-DD-N-slug.plan.md`).

### Public names

```rust
// crates/gateway-client/src/lib.rs   NEW

//! A client for chat-completions servers, plus the few opt-in extras that the Runvil Gateway adds.

mod config;
mod error;
mod failure;
mod message;
mod models;
mod options;
mod reply;
mod transport;
mod wire;

pub use config::{GatewayEndpoint, SecretString};
pub use error::{Error, ErrorKind};
pub use message::{Message, Role};
pub use models::{Model, Thinking, fetch_models};
pub use options::{Options, Tool};
pub use reply::{ClientTiming, Outcome, Reply, ToolCall, Usage};
pub use transport::GatewayChat;
pub use wire::delta::StreamDelta;
pub use wire::stream::escape_controls;
```

That is 19 public names. Today's crate has 15, of which 9 leave for the adapter or become private (Appendix B of the report). `Role` joins the list because `Message::text` takes it. The report's Appendix A aims for about 11, which needs the `Chat` builder and is a later cleanup. Confidence: medium.

Private after the move, as no code outside the crate calls them today: `ChunkSource`, `read_body_capped`, `read_completion_stream`, `build_request_body`, `make_classify_http_failure_error`, `make_classify_stream_error`. In `wire.rs` they become `pub(crate)`.

## Part 2. The client's own types (all NEW)

### `message.rs`

The Engine's `Message` serializes to `role`, `content`, and optionally `tool_call_id` and `tool_calls` (wire.rs). The client's `Message` writes the same JSON for the cases a stranger needs. Echoing an assistant turn that made tool calls (`tool_calls`) is left for later, because the Runvil path sends the Engine's own `Message`.

```rust
use serde::Serialize;
use serde_json::Value;

/// Who speaks a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Instructions that frame the conversation.
    System,
    /// A turn from the user.
    User,
    /// A turn from the model.
    Assistant,
    /// The result of a tool call the model requested.
    Tool,
}

/// One chat-completions message.
///
/// It writes the same JSON as the Runvil Engine's own message type. A test in
/// the Harness adapter keeps the two identical.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    role: Role,
    content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

impl Message {
    /// A text message from `role`. A tool result also names the call it answers, so it has its
    /// own constructor, [`Message::tool`].
    #[must_use]
    pub fn text(role: Role, content: impl Into<String>) -> Message {
        Message {
            role,
            content: Value::String(content.into()),
            tool_call_id: None,
        }
    }

    /// A `tool` message holding the result of the call named by `tool_call_id`.
    #[must_use]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Message {
        Message {
            role: Role::Tool,
            content: Value::String(content.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}
```

### `options.rs`

`Tool` mirrors the Engine's `ToolSchema` getters. `Options` mirrors what `build_request_body` reads from `CompletionOptions` today: `model()`, `temperature()`, `max_tokens()`, and `thinking()`. The client does not validate the temperature range or the tool name. The Engine already did when it built its own values, and a server refuses the rest.

Where `parameters` comes from, since the review asked: each tool's `parameters_schema` in the Engine's tool catalog (info.rs), documented there as "the JSON-Schema `object` the tool's arguments must match". The tool's author writes it as JSON: a C-contract Plugin passes it across the ABI as text, and Lua's `tools.adhoc` converts a Lua table. The Engine checks only that it is an object and that the tool name matches `[A-Za-z][A-Za-z0-9_-]{0,63}` (`tool_schema_new` in `detail.rs`). The client's `Map<String, Value>` holds the same guarantee in the type. A typed model of JSON Schema is left out on purpose: it would either drop keywords that servers accept (`oneOf`, `$ref`, `enum`, `additionalProperties`) or reimplement the standard.

```rust
use std::num::NonZeroU32;

use serde_json::{Map, Value};

/// A tool offered to the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    name: String,
    description: String,
    parameters: Map<String, Value>,
}

impl Tool {
    /// `parameters` is the JSON Schema object that describes the tool's arguments, for example
    /// `{"type": "object", "properties": {...}, "required": [...]}`. The server defines what
    /// else it may hold. The type says it is an object, which is the one rule every server
    /// shares, and leaves the schema's keywords open.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Map<String, Value>,
    ) -> Tool {
        Tool {
            name: name.into(),
            description: description.into(),
            parameters,
        }
    }

    /// The name the model calls the tool by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The sentence that tells the model what the tool does.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The JSON Schema object for the tool's arguments.
    #[must_use]
    pub fn parameters(&self) -> &Map<String, Value> {
        &self.parameters
    }
}

/// The request settings of one round.
#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    model: String,
    temperature: Option<f64>,
    max_tokens: Option<NonZeroU32>,
    thinking: Option<bool>,
}

impl Options {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Options {
        Options { model: model.into(), temperature: None, max_tokens: None, thinking: None }
    }

    #[must_use]
    pub fn with_temperature(mut self, temperature: f64) -> Options {
        self.temperature = Some(temperature);
        self
    }

    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: NonZeroU32) -> Options {
        self.max_tokens = Some(max_tokens);
        self
    }

    #[must_use]
    pub fn with_thinking(mut self, thinking: bool) -> Options {
        self.thinking = Some(thinking);
        self
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn temperature(&self) -> Option<f64> {
        self.temperature
    }

    #[must_use]
    pub fn max_tokens(&self) -> Option<NonZeroU32> {
        self.max_tokens
    }

    #[must_use]
    pub fn thinking(&self) -> Option<bool> {
        self.thinking
    }
}
```

### `reply.rs`

`Reply` holds what `Completion` holds today, minus the llama.cpp and vLLM metrics, which the adapter reads from `response`. The four tool-call checks are the Engine's `check_call_id`, `check_call_name`, `checked_call_arguments`, and `check_unique_call_id` (normalize.rs), with the same message text. The client raises them as `MalformedResponse` directly. Today the Engine raises `InvalidParts` and `decoded()` converts it to the same `MalformedResponse` text, so the visible result is identical.

```rust
use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::error::Error;

/// One tool call the model asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    id: String,
    name: String,
    arguments: Map<String, Value>,
}

impl ToolCall {
    /// # Errors
    /// `MalformedResponse` when `id` or `name` is blank or `arguments` is not a JSON object.
    pub(crate) fn from_parts(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Value,
    ) -> Result<ToolCall, Error> {
        let id = id.into();
        let name = name.into();
        if id.trim().is_empty() {
            return Err(Error::malformed("tool call id was blank"));
        }
        if name.trim().is_empty() {
            return Err(Error::malformed("tool call name was blank"));
        }
        let Value::Object(arguments) = arguments else {
            return Err(Error::malformed("tool call arguments were not a JSON object"));
        };
        Ok(ToolCall { id, name, arguments })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn arguments(&self) -> &Map<String, Value> {
        &self.arguments
    }
}

/// What the model produced: an answer or a request to call tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

/// Token counts as the server reported them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    pub cached_tokens: Option<u32>,
    pub reasoning_tokens: Option<u32>,
}

impl Usage {
    #[must_use]
    pub fn new(prompt_tokens: u32, completion_tokens: u32, total_tokens: u32) -> Usage {
        Usage { prompt_tokens, completion_tokens, total_tokens, cached_tokens: None, reasoning_tokens: None }
    }

    #[must_use]
    pub fn with_cached_tokens(mut self, cached_tokens: Option<u32>) -> Usage {
        self.cached_tokens = cached_tokens;
        self
    }

    #[must_use]
    pub fn with_reasoning_tokens(mut self, reasoning_tokens: Option<u32>) -> Usage {
        self.reasoning_tokens = reasoning_tokens;
        self
    }
}

/// Timing the client measured on its own clock.
#[derive(Debug, Clone, PartialEq)]
pub struct ClientTiming {
    pub ttft_ms: Option<f64>,
    pub mean_itl_ms: Option<f64>,
    pub e2e_ms: f64,
}

impl ClientTiming {
    #[must_use]
    pub fn new(e2e_ms: f64) -> ClientTiming {
        ClientTiming { ttft_ms: None, mean_itl_ms: None, e2e_ms }
    }

    #[must_use]
    pub fn with_ttft_ms(mut self, ttft_ms: Option<f64>) -> ClientTiming {
        self.ttft_ms = ttft_ms;
        self
    }

    #[must_use]
    pub fn with_mean_itl_ms(mut self, mean_itl_ms: Option<f64>) -> ClientTiming {
        self.mean_itl_ms = mean_itl_ms;
        self
    }
}

/// The finished reply of one round.
#[derive(Debug, Clone, PartialEq)]
pub struct Reply {
    outcome: Outcome,
    model: String,
    finish_reason: Option<String>,
    reasoning: Option<String>,
    usage: Option<Usage>,
    timing: Option<ClientTiming>,
    diagnostics: Vec<String>,
    request: Value,
    response: Value,
}

impl Reply {
    /// The two checks `Completion::from_result` runs today: an empty tool-call batch is an
    /// `EmptyReply`, and two calls sharing an id are a `MalformedResponse`.
    pub(crate) fn new(
        outcome: Outcome,
        model: impl Into<String>,
        request: Value,
        response: Value,
    ) -> Result<Reply, Error> {
        if let Outcome::ToolCalls(calls) = &outcome {
            if calls.is_empty() {
                return Err(Error::empty_reply(false));
            }
            let mut seen = HashSet::new();
            for call in calls {
                if !seen.insert(call.id()) {
                    return Err(Error::malformed(format!(
                        "duplicate tool call id {:?} within one turn",
                        call.id()
                    )));
                }
            }
        }
        Ok(Reply {
            outcome,
            model: model.into(),
            finish_reason: None,
            reasoning: None,
            usage: None,
            timing: None,
            diagnostics: Vec::new(),
            request,
            response,
        })
    }

    pub(crate) fn with_finish_reason(mut self, finish_reason: String) -> Reply {
        self.finish_reason = Some(finish_reason);
        self
    }

    pub(crate) fn with_reasoning(mut self, reasoning: String) -> Reply {
        self.reasoning = Some(reasoning);
        self
    }

    pub(crate) fn with_usage(mut self, usage: Option<Usage>) -> Reply {
        self.usage = usage;
        self
    }

    pub(crate) fn with_timing(mut self, timing: Option<ClientTiming>) -> Reply {
        self.timing = timing;
        self
    }

    pub(crate) fn with_diagnostics(mut self, diagnostics: Vec<String>) -> Reply {
        self.diagnostics = diagnostics;
        self
    }

    #[must_use]
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// The model the request named, or the one the response named when the request named none.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub fn finish_reason(&self) -> Option<&str> {
        self.finish_reason.as_deref()
    }

    /// Reasoning text from the server. It is never promoted into the answer.
    #[must_use]
    pub fn reasoning(&self) -> Option<&str> {
        self.reasoning.as_deref()
    }

    #[must_use]
    pub fn usage(&self) -> Option<&Usage> {
        self.usage.as_ref()
    }

    #[must_use]
    pub fn timing(&self) -> Option<&ClientTiming> {
        self.timing.as_ref()
    }

    /// One line for each part of the response that was present but unusable, and for a response
    /// that named no model.
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// The request body that was sent.
    #[must_use]
    pub fn request(&self) -> &Value {
        &self.request
    }

    /// The response, reassembled into the buffered chat-completion shape when it streamed.
    #[must_use]
    pub fn response(&self) -> &Value {
        &self.response
    }
}
```

## Part 3. Errors (NEW `error.rs`, CHANGED `failure.rs` and `config.rs`)

Today every failure is a `runvil::Error` built with `make_completion_error(code, phrase(code))`. The Engine's `Error` has a `message` for logs, a `prose` that a model may read, and `facts`. The client's `Error` keeps the same two texts and the same values, as plain fields, so the adapter's mapping is a copy and no wording changes. The client says `summary` where the Engine says `prose`.

The 12 phrases below are the Engine's `phrase()` table (error.rs) copied word for word. A test in the adapter fails if the two tables ever differ.

```rust
// crates/gateway-client/src/error.rs   NEW

use std::fmt;

/// What went wrong, in the terms a caller acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    ContextOverflow,
    RateLimited,
    QuotaExhausted,
    Overloaded,
    Refused,
    Timeout,
    Transport,
    ServerError,
    Rejected,
    MalformedResponse,
    EmptyReply,
    Unavailable,
    /// The caller's setup is wrong: a bad URL, a missing variable, an empty key.
    InvalidConfig,
    /// The messages could not be written as chat messages.
    InvalidRequest,
}

impl ErrorKind {
    /// Every kind, in declaration order, for tests that must visit them all. The match in
    /// `phrase` has no wildcard arm, so adding a kind fails to compile until it is handled
    /// there. This list is the one place the compiler cannot check, as with the Engine's
    /// `ModelRole::ALL`.
    pub const ALL: [ErrorKind; 14] = [
        ErrorKind::ContextOverflow,
        ErrorKind::RateLimited,
        ErrorKind::QuotaExhausted,
        ErrorKind::Overloaded,
        ErrorKind::Refused,
        ErrorKind::Timeout,
        ErrorKind::Transport,
        ErrorKind::ServerError,
        ErrorKind::Rejected,
        ErrorKind::MalformedResponse,
        ErrorKind::EmptyReply,
        ErrorKind::Unavailable,
        ErrorKind::InvalidConfig,
        ErrorKind::InvalidRequest,
    ];

    /// The fixed wording of the kind. It never holds server text.
    #[must_use]
    pub const fn phrase(self) -> &'static str {
        match self {
            ErrorKind::ContextOverflow => "the request is larger than the model's context window",
            ErrorKind::RateLimited => "the model backend is limiting the request rate",
            ErrorKind::QuotaExhausted => "the model backend says the usage quota is spent",
            ErrorKind::Overloaded => "the model backend is overloaded",
            ErrorKind::Refused => "the model backend refused the request on content policy grounds",
            ErrorKind::Timeout => "the model backend did not answer in time",
            ErrorKind::Transport => "the connection to the model backend failed",
            ErrorKind::ServerError => "the model backend reported a fault of its own",
            ErrorKind::Rejected => "the model backend rejected the request",
            ErrorKind::MalformedResponse => {
                "the model backend sent a reply that could not be understood"
            }
            ErrorKind::EmptyReply => "the model replied with no text and no tool calls",
            ErrorKind::Unavailable => "model access is turned off or not configured",
            ErrorKind::InvalidConfig => "the client is not set up correctly",
            ErrorKind::InvalidRequest => "the request messages could not be sent as chat messages",
        }
    }
}

/// A failed round, model list, or setup step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    /// The full diagnostic for logs: the summary, then server text and causes.
    pub message: String,
    /// Wording that is safe to show a model. It holds this crate's text and never server text.
    pub summary: String,
    /// The HTTP status of a failed response, when there was one.
    pub status: Option<u16>,
    /// The `finish_reason` of an empty reply.
    pub finish_reason: Option<String>,
    /// Tokens in the prompt, when a context overflow named them.
    pub prompt_tokens: Option<u32>,
    /// The model's context window, when a context overflow named it.
    pub context_window: Option<u32>,
    /// Which setup rule failed, for `InvalidConfig`.
    pub reason: Option<&'static str>,
    /// The environment variable a setup error names.
    pub name: Option<String>,
}

impl Error {
    /// `message` is both the message and the summary, as `make_completion_error` does today.
    pub(crate) fn new(kind: ErrorKind, message: impl Into<String>) -> Error {
        let message = message.into();
        Error {
            kind,
            summary: message.clone(),
            message,
            status: None,
            finish_reason: None,
            prompt_tokens: None,
            context_window: None,
            reason: None,
            name: None,
        }
    }

    /// The kind's fixed phrase alone.
    pub(crate) fn phrased(kind: ErrorKind) -> Error {
        Error::new(kind, kind.phrase())
    }

    /// The kind's phrase, then `: ` and `specific`, which this crate wrote.
    pub(crate) fn specific(kind: ErrorKind, specific: impl fmt::Display) -> Error {
        Error::new(kind, format!("{}: {specific}", kind.phrase()))
    }

    pub(crate) fn malformed(specific: impl fmt::Display) -> Error {
        Error::specific(ErrorKind::MalformedResponse, specific)
    }

    pub(crate) fn invalid_request(specific: impl fmt::Display) -> Error {
        Error::specific(ErrorKind::InvalidRequest, specific)
    }

    pub(crate) fn unavailable() -> Error {
        Error::phrased(ErrorKind::Unavailable)
    }

    /// The text matches `make_empty_reply_error(reasoning_present)` in the Engine.
    pub(crate) fn empty_reply(reasoning_present: bool) -> Error {
        if reasoning_present {
            Error::specific(ErrorKind::EmptyReply, "reasoning content was present but ignored")
        } else {
            Error::phrased(ErrorKind::EmptyReply)
        }
    }

    pub(crate) fn context_overflow(
        prompt_tokens: Option<u32>,
        context_window: Option<u32>,
        message: impl Into<String>,
    ) -> Error {
        let mut error = Error::new(ErrorKind::ContextOverflow, message);
        error.prompt_tokens = prompt_tokens;
        error.context_window = context_window;
        error
    }

    /// A setup failure. `reason` is one of `missing_env`, `invalid_env`, `invalid_config`,
    /// `empty_secret`, the values the Engine's `Reason` fact holds today.
    pub(crate) fn setup(reason: &'static str, message: impl Into<String>) -> Error {
        let mut error = Error::new(ErrorKind::InvalidConfig, message);
        error.reason = Some(reason);
        error
    }

    pub(crate) fn with_summary(mut self, summary: impl Into<String>) -> Error {
        self.summary = summary.into();
        self
    }

    /// Appends server text to the message after `: `. The summary stays as it was.
    pub(crate) fn with_detail(mut self, text: &str) -> Error {
        self.message.push_str(": ");
        self.message.push_str(text);
        self
    }

    /// Appends `cause`, and each cause under it, to the message. This is the Engine's
    /// `Error::with_cause` (crates/runvil-internal/types/src/error.rs in runvil-harness),
    /// copied without change.
    pub(crate) fn with_cause(mut self, cause: &(dyn std::error::Error + 'static)) -> Error {
        let mut next = Some(cause);
        while let Some(current) = next {
            let text = current.to_string();
            if !text.is_empty() && !self.message.contains(&text) {
                let repeated = text
                    .rmatch_indices(": ")
                    .map(|(at, _)| at)
                    .find(|&at| at > 0 && self.message.ends_with(&text[..at]));
                if let Some(at) = repeated {
                    self.message.push_str(&text[at..]);
                } else {
                    self.message.push_str(": ");
                    self.message.push_str(&text);
                }
            }
            next = current.source();
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary)
    }
}

impl std::error::Error for Error {}
```

`failure.rs` keeps the same function names, so every call site in the moved files stays as written. Only the bodies change.

```rust
// crates/gateway-client/src/failure.rs   CHANGED

use std::fmt::Display;

use crate::error::{Error, ErrorKind};

#[expect(
    clippy::needless_pass_by_value,
    reason = "a `map_err` adapter takes the error by value"
)]
pub(crate) fn make_transport_failure_error(error: reqwest::Error) -> Error {
    if error.is_timeout() {
        return make_timeout_error(&error);
    }
    Error::phrased(ErrorKind::Transport).with_cause(&error)
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "a `map_err` adapter takes the error by value"
)]
pub(crate) fn make_elapsed_error(error: tokio::time::error::Elapsed) -> Error {
    make_timeout_error(&error)
}

fn make_timeout_error(cause: &(dyn std::error::Error + 'static)) -> Error {
    Error::phrased(ErrorKind::Timeout).with_cause(cause)
}

pub(crate) fn make_unavailable_error() -> Error {
    Error::unavailable()
}

pub(crate) fn make_malformed_error(specific: impl Display) -> Error {
    Error::malformed(specific)
}

pub(crate) fn with_detail(error: Error, text: &str) -> Error {
    error.with_detail(text)
}
```

`decoded()` is deleted. The client builds `MalformedResponse` itself, so nothing needs converting.

`config.rs` changes. `GatewayEndpoint` and `SecretString` keep their logic. Four edits:

```rust
// crates/gateway-client/src/config.rs   CHANGED (only these items differ)

use crate::error::Error;

pub(crate) fn make_missing_env_error(name: &str) -> Error {
    let mut error = Error::setup("missing_env", format!("missing environment variable: {name}"));
    error.name = Some(name.to_owned());
    error
}

pub(crate) fn make_invalid_env_error(name: &str) -> Error {
    let mut error = Error::setup(
        "invalid_env",
        format!("environment variable is set but not valid Unicode: {name}"),
    );
    error.name = Some(name.to_owned());
    error
}

// SecretString::new: message and summary differ, as they do today.
pub fn new(secret: impl Into<String>) -> std::result::Result<SecretString, Error> {
    let secret = secret.into();
    if secret.is_empty() {
        return Err(Error::setup("empty_secret", "secret must not be empty")
            .with_summary("The secret is empty. Supply a non-empty secret."));
    }
    Ok(SecretString(secret))
}

// Was `pub(crate) fn expose`. The adapter writes the key into the Plugin entry, so it is public.
/// Borrows the raw secret. The key is read in exactly two places: the `Authorization` header
/// of a request, and the adapter's `runvil/web-search` entry.
#[must_use]
pub fn expose_secret(&self) -> &str {
    &self.0
}

// GatewayEndpoint::new: `reject` becomes
let reject = |message: String| Error::setup("invalid_config", message);
```

The `lib.rs` invariant list in today's crate says "the key leaves the crate in two places only". That sentence moves into the doc of `expose_secret` above and is true of the two crates together.

## Part 4. The wire code: what changes in each moved file

Every moved file gets the same five mechanical edits: `use runvil::...` lines become `use crate::...`, `runvil::Error` becomes `crate::error::Error`, `ErrorCode::X` becomes `ErrorKind::X`, `error.facts.insert(ErrorFact::X, ...)` becomes an assignment to the matching field, and every `#[non_exhaustive]` is deleted (today it is on `StreamDelta`, `SecretString`, `GatewayEndpoint`, and `GatewayChat`). The porting rule counts the last one as an allowed edit. The sections below show the places that need more than that.

### `wire/request.rs` (CHANGED: generic over the messages)

Today the body is built by `serde_json::json!` with `"messages": messages`. That line serializes `&[Message]`, and `json!` panics if a type's `Serialize` fails. For a type the caller chooses, the client has to turn that into an error. It also checks that every item has a string `role`, so a wrong shape fails here and not at the server.

```rust
use serde::Serialize;
use serde_json::Value;

use crate::error::Error;
use crate::options::{Options, Tool};

pub(crate) fn build_request_body<M: Serialize>(
    messages: &[M],
    tools: Option<&[Tool]>,
    options: &Options,
) -> Result<Value, Error> {
    let messages = serde_json::to_value(messages).map_err(|error| {
        Error::invalid_request("the messages could not be written as JSON").with_cause(&error)
    })?;
    check_messages(&messages)?;
    let mut body = serde_json::json!({
        "model": options.model(),
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    if let Some(tools) = tools.filter(|tools| !tools.is_empty()) {
        let wrapped: Vec<Value> = tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name(),
                        "description": tool.description(),
                        "parameters": tool.parameters(),
                    },
                })
            })
            .collect();
        body["tools"] = Value::Array(wrapped);
        body["tool_choice"] = Value::String("auto".into());
    }
    if let Some(temperature) = options.temperature() {
        body["temperature"] = serde_json::json!(temperature);
    }
    if let Some(max_tokens) = options.max_tokens() {
        body["max_tokens"] = serde_json::json!(max_tokens.get());
    }
    if let Some(thinking) = options.thinking() {
        body["chat_template_kwargs"] = serde_json::json!({
            "enable_thinking": thinking,
        });
    }
    Ok(body)
}

/// The messages must write as a JSON array of objects that each have a string `role`.
fn check_messages(messages: &Value) -> Result<(), Error> {
    let Value::Array(items) = messages else {
        return Err(Error::invalid_request("the messages did not write as a JSON array"));
    };
    for (index, item) in items.iter().enumerate() {
        if !matches!(item.get("role"), Some(Value::String(_))) {
            return Err(Error::invalid_request(format!(
                "message {index} has no string `role`"
            )));
        }
    }
    Ok(())
}
```

Differences from today, so a reviewer can check them: the function returns `Result`; `temperature` was `temperature.get()` on the Engine's `Temperature` newtype and is now the plain `f64`; `Message` and `ToolSchema` became the generic `M` and the client's `Tool`. The JSON keys and their order are unchanged. The `request-tests.rs` expectations must not change (Part 7).

### `wire/classify.rs` (CHANGED: types only)

The rules, phrase lists, and `overflow_counts` helpers are copied unchanged. These two functions and `make_http_error` change because they built Engine values.

```rust
use crate::error::{Error, ErrorKind};
use crate::failure::with_detail;

#[must_use]
pub(crate) fn make_classify_http_failure_error(status: u16, body: &str) -> Error {
    let lower = body.to_lowercase();
    let http = |kind: ErrorKind| with_detail(make_http_error(kind, kind.phrase(), status), body);
    if matches!(status, 400 | 413) && names_any(&lower, OVERFLOW_PHRASES) {
        let (prompt_tokens, window) = overflow_counts(&lower);
        let overflow = Error::context_overflow(
            prompt_tokens,
            window,
            format!("{} (status {status})", ErrorKind::ContextOverflow.phrase()),
        );
        return with_detail(overflow, body);
    }
    match status {
        429 if names_any(&lower, QUOTA_WORDS) => http(ErrorKind::QuotaExhausted),
        429 => http(ErrorKind::RateLimited),
        503 | 529 => http(ErrorKind::Overloaded),
        500..=599 if lower.contains("overloaded") => http(ErrorKind::Overloaded),
        500..=599 => http(ErrorKind::ServerError),
        401 | 403 => with_detail(
            make_http_error(ErrorKind::Unavailable, CREDENTIALS_PHRASE, status),
            body,
        ),
        400 if names_any(&lower, REFUSAL_WORDS) => http(ErrorKind::Refused),
        _ => http(ErrorKind::Rejected),
    }
}

#[must_use]
pub(crate) fn make_classify_stream_error(body: &str) -> Error {
    let lower = body.to_lowercase();
    let error = if names_any(&lower, OVERFLOW_PHRASES) {
        let (prompt_tokens, window) = overflow_counts(&lower);
        Error::context_overflow(prompt_tokens, window, ErrorKind::ContextOverflow.phrase())
    } else {
        let kind = if names_any(&lower, QUOTA_WORDS) {
            ErrorKind::QuotaExhausted
        } else if lower.contains("overloaded") {
            ErrorKind::Overloaded
        } else if names_any(&lower, REFUSAL_WORDS) {
            ErrorKind::Refused
        } else {
            ErrorKind::Transport
        };
        Error::phrased(kind)
    };
    with_detail(error, body)
}

fn make_http_error(kind: ErrorKind, phrase: &str, status: u16) -> Error {
    let mut error = Error::new(kind, format!("{phrase} (status {status})"));
    error.status = Some(status);
    error
}
```

### `wire/parse.rs` (CHANGED: three edits)

1. `normalize` and `parse_openai_tool_calls`: `ToolCall::from_parts(id, name, arguments).map_err(decoded)?` becomes `ToolCall::from_parts(id, name, arguments)?`, because the client's `from_parts` already raises `MalformedResponse`. `CompletionResult` becomes `Outcome`. The empty-reply branch becomes:

```rust
let mut error = Error::empty_reply(reasoning_content.is_some());
if let Some(reason) = finish_reason {
    error.finish_reason = Some(reason);
}
Err(error)
```

2. `response_metadata` keeps the model and `usage`. The llama.cpp and vLLM sections leave (they move to the adapter, Part 5). The order of the diagnostics stays model, usage, so the adapter's later lines for timings and metrics keep today's order.

```rust
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ResponseMetadata {
    pub(super) model: String,
    pub(super) usage: Option<Usage>,
    pub(super) diagnostics: Vec<String>,
}

pub(super) fn response_metadata(body: &Value) -> ResponseMetadata {
    let mut diagnostics = Vec::new();
    ResponseMetadata {
        model: parse_model(body, &mut diagnostics),
        usage: parse_section(body, "usage", parse_usage, &mut diagnostics),
        diagnostics,
    }
}

fn parse_usage(value: &Value) -> Result<Usage, serde_json::Error> {
    let wire = WireUsage::deserialize(value)?;
    Ok(Usage::new(wire.prompt_tokens, wire.completion_tokens, wire.total_tokens)
        .with_cached_tokens(wire.prompt_tokens_details.and_then(|details| details.cached_tokens))
        .with_reasoning_tokens(
            wire.completion_tokens_details.and_then(|details| details.reasoning_tokens),
        ))
}
```

3. `WireLlamaTimings`, `parse_llama_timings`, and `parse_vllm_metrics` are cut from this file and pasted into the adapter's `metrics.rs`. `parse_section` stays here and is copied into the adapter too.

### `wire/stream.rs` (CHANGED: `finish`)

The truncation rule before `normalize` is unchanged. The lines after it change because the result is a `Reply`. The Engine's `measured` test moves to the adapter, which now builds the metrics.

```rust
pub(super) fn finish(
    self,
    request_body: Value,
    timing: Option<ClientTiming>,
) -> Result<Reply, Error> {
    // The truncation rule: unchanged.
    let response_body = self.into_body();
    let turn = normalize(&response_body)?;
    let metadata = response_metadata(&response_body);
    let model = match request_body.get("model") {
        Some(Value::String(model)) => model.clone(),
        _ => metadata.model,
    };
    let mut reply = Reply::new(turn.outcome, model, request_body, response_body)?
        .with_diagnostics(metadata.diagnostics)
        .with_usage(metadata.usage)
        .with_timing(timing);
    if let Some(reason) = turn.finish_reason {
        reply = reply.with_finish_reason(reason);
    }
    if let Some(reasoning) = turn.reasoning_content {
        reply = reply.with_reasoning(reasoning);
    }
    Ok(reply)
}
```

### `wire/read.rs` (CHANGED: types only)

`ChunkSource::next_chunk` returns the client's `Error`. `read_completion_stream` returns `Result<Reply, Error>` and builds the client's `ClientTiming`. The byte cap, the `[DONE]` rule, and the timing arithmetic are unchanged.

### `transport.rs` (CHANGED: `complete`)

```rust
pub async fn complete<M: Serialize + Sync>(
    &self,
    messages: &[M],
    tools: Option<&[Tool]>,
    options: &Options,
    on_delta: impl Fn(StreamDelta),
) -> Result<Reply, Error> {
    let GatewayTransport::Http(http) = &self.transport else {
        return Err(make_unavailable_error());
    };
    let request_body = build_request_body(messages, tools, options)?;
    // The rest of the body is unchanged, except `key.expose()` is now `key.expose_secret()`.
```

`M: Sync` is needed because the adapter's future must be `Send`, and a `&[M]` held in it is `Send` only when `M` is `Sync`. The Engine's `Message` is `Sync`. `from_env` and the other constructors are unchanged in Stages A and B, and Stage C replaces `from_env` with `Gateway::find_env`.

### `models.rs` (MOVED from `catalog.rs`: stops at the list)

The fetch, the byte caps, and the error classification are unchanged. The part that built `ModelId`, `ModelInfo`, and `ModelCatalog` leaves for the adapter, so the client returns the list as it came. An entry without `context` stays in the list, which is what a stock OpenAI server needs. The adapter applies today's rules to it.

```rust
use serde::Deserialize;

/// How a model handles thinking tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Thinking {
    Never,
    Always,
    Switchable,
}

/// One entry of `GET /v1/models`. The Gateway fills every field. A stock server fills `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub id: String,
    pub description: String,
    pub context: Option<u32>,
    pub thinking: Option<Thinking>,
    pub provider: Option<String>,
}

pub async fn fetch_models(base_url: &str, key: &SecretString) -> Result<Vec<Model>, Error> {
    let base = base_url.trim_end_matches('/');
    let response = get_authed(format!("{base}/models"), key.expose_secret()).await?;
    let body = read_catalog_body_capped(response, MAX_CATALOG_BODY).await?;
    let list: ModelsListResponse = serde_json::from_slice(&body).map_err(|error| {
        make_malformed_error("model list response was not valid JSON").with_cause(&error)
    })?;
    Ok(list
        .data
        .into_iter()
        .map(|entry| Model {
            id: entry.id,
            description: entry.description,
            context: entry.context,
            thinking: entry.thinking,
            provider: entry.provider,
        })
        .collect())
}
```

`ModelsListEntry.thinking` changes from the Engine's `ThinkingMode` to `Option<Thinking>`. The values on the wire (`never`, `always`, `switchable`) are the same.

One signature change on purpose: today's `fetch_model_catalog(base_url, token: &str)` takes the key as a plain `&str`, so the broker has to expose it. `fetch_models` takes `&SecretString`, so the adapter never reads the key to list models, and the key is read inside the client only. `get_authed` and the other three helpers (`read_catalog_body_capped`, `read_error_body_bounded`, `catalog_client`) are moved unchanged.

## Part 6. Stage C: discovery and the `Gateway` type (NEW, after the move)

This part is not needed for the adapter to work. It lands in the new repository as pull request 2, before the Harness pull request, so the Harness pins the final crate once (Stage C in Part 7 and the merge order in Delivery).

### What moves from the Gateway's `gateway-api-discovery` crate, and what does not

The reader half moves into `src/discovery/` behind the `discovery` feature. The rest stays where it is.

```text
src/discovery/                 (feature = "discovery")
  mod.rs          NEW      read_live(), default_run_dir(), check_version()
  file.rs         MOVED    DiscoveryFile: Deserialize only. Keeps read(), validation_error(), local_addr().
                           Drops Serialize, write_to(), remove_if_mine().
  paths.rs        MOVED    run_dir(), default_run_dir(), the gateway.json file name. Drops the lock and log paths.
  liveness.rs     MOVED    validated.rs, renamed. prove_live() was ValidatedConnection::validate(): it proves the
                           discovery file names a live, authorized Gateway (pid alive, image name, boot identity,
                           same-socket health and bearer proof). Drops the *_cancellable variants.
  health.rs       MOVED    the probe functions. Drops wait_for_health (the launcher's).
  health/connection.rs  MOVED
  sys.rs, sys/{linux,macos,windows}.rs  MOVED   process identity: 255 lines, 7 unsafe blocks, each already under #[expect(unsafe_code)]
  error.rs        CHANGED  SidecarError is replaced by plain Error values (below)

stays in runvil-gateway:  atomic.rs, write_to, remove_if_mine, GatewayInstanceLease (in lock.rs)
stays with Workshop:      LaunchLock, LaunchDecision, launch_or_attach*, shutdown.rs
dropped (no user):        cancellation.rs, resolve_cancellable, test-fixtures feature of the old crate
```

The old crate's `error.rs` uses `shared-error-source` and its `build.rs` uses `build-ceiling`. Both are path dependencies inside the Gateway workspace, so neither moves. The moved code derives its errors with `thiserror`, which is already in the Harness lockfile, kept out of builds that turn the feature off:

```toml
# crates/gateway-client/Cargo.toml   Stage C additions
[features]
default = ["discovery"]
discovery = ["dep:thiserror", "dep:libc", "dep:windows-sys"]

[dependencies]
thiserror = { workspace = true, optional = true }          # workspace: thiserror = "2"

[target.'cfg(target_os = "macos")'.dependencies]
libc = { version = "0.2", optional = true }

[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.61", default-features = false, features = ["Win32_Foundation", "Win32_System_Threading"], optional = true }
```

The two versions are the Gateway workspace's own.

### `discovery/mod.rs` (NEW)

`find` only reads. The Gateway crate's `resolve` deletes a file it judges stale, and its own comments say deleting is the lock holder's privilege. This reader is often not the lock holder and may race the Gateway's atomic write, so it reports a stale file and leaves it alone. The Gateway removes its own file on exit, and Workshop's launcher cleans up with its own copy of `resolve`.

```rust
use std::path::{Path, PathBuf};

use crate::error::{Error, ErrorKind};

mod file;
mod health;
mod liveness;
mod paths;
mod sys;

pub(crate) use file::DiscoveryFile;

/// The run directory a Gateway writes `gateway.json` into: `<home>/.runvil/run`.
pub(crate) fn default_run_dir() -> Option<PathBuf> {
    paths::default_run_dir()
}

/// Reads `gateway.json` in `run_dir` and proves it names a live Gateway.
///
/// `Ok(None)` means there is no file. A file that fails validation is an error that says why.
/// The file is never deleted.
pub(crate) fn read_live(run_dir: &Path) -> Result<Option<DiscoveryFile>, Error> {
    let file = match DiscoveryFile::read(run_dir) {
        Ok(Some(file)) => file,
        Ok(None) => return Ok(None),
        Err(error) => return Err(Error::specific(ErrorKind::Unavailable, error)),
    };
    match liveness::prove_live(file) {
        Ok(file) => Ok(Some(file)),
        Err(reason) => Err(Error::specific(
            ErrorKind::Unavailable,
            format!("the gateway discovery file is stale: {reason}"),
        )),
    }
}

/// The oldest Gateway this client works with, as (major, minor, patch). Set it from the
/// Gateway's own version at the time of the move.
const MIN_GATEWAY_VERSION: (u64, u64, u64) = (0, 1, 0);

pub(crate) fn check_version(version: &str) -> Result<(), Error> {
    let Some(found) = parse_version(version) else {
        return Err(Error::specific(
            ErrorKind::Unavailable,
            format!("the gateway reports version {version:?}, which is not major.minor.patch"),
        ));
    };
    if found < MIN_GATEWAY_VERSION {
        let (major, minor, patch) = MIN_GATEWAY_VERSION;
        return Err(Error::specific(
            ErrorKind::Unavailable,
            format!("the gateway is too old: it is {version}, and this client needs {major}.{minor}.{patch} or newer"),
        ));
    }
    Ok(())
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some((major, minor, patch))
}
```

`liveness::prove_live` is the moved `ValidatedConnection::validate`, which returns the `DiscoveryFile` on success and a `StaleReason` (a `thiserror` enum with `Display`) on failure. It still requires the process image `runvil-gateway`. The `Display` of `file::read`'s error is the moved read, parse, and invalid-field cases.

The review asked for a more descriptive name than `validate`. The function does more than check a file: it opens one socket and proves that the Gateway behind the file is running and accepts the key. `DiscoveryFile::validation_error()` already means something narrower (the static field checks: port not zero, key not empty, pid not zero), so `validate_discovery_file` would name the wrong check. The old module's own doc calls the result "a Gateway connection proven live and authorized", so the function is `prove_live` and the module is `liveness`.

### `gateway.rs` (NEW)

This is the type behind the thread's `Gateway::find()`, `find_env()`, and `find_env_named(...)`. Rust has no overloading, so the env function with names is a separate name. `from_lookup` is today's `from_env_with` with the variable names as parameters, and `GatewayChat::from_env` is deleted once this lands.

`lib.rs` gains two modules and one export. The workspace lint `unreachable_pub` warns about a `pub` item that no public path reaches, so `Gateway` must be re-exported. The public names go from 19 to 20.

```rust
// crates/gateway-client/src/lib.rs   Stage C additions
#[cfg(feature = "discovery")]
mod discovery;
mod gateway;

pub use gateway::Gateway;
```

Each `pub fn` that returns `Result` needs a `# Errors` section in its doc comment (clippy pedantic `missing_errors_doc`). The blocks below leave them out to stay short.

```rust
use std::path::Path;

use crate::config::{GatewayEndpoint, SecretString, make_invalid_env_error, make_missing_env_error};
use crate::error::{Error, ErrorKind};
use crate::transport::GatewayChat;

const DEFAULT_URL_VAR: &str = "RUNVIL_GATEWAY_URL";
const DEFAULT_KEY_VAR: &str = "RUNVIL_GATEWAY_API_KEY";

/// Where a Gateway is and how to log in: an API root and, when it needs one, a bearer key.
#[derive(Clone, Debug)]
pub struct Gateway {
    endpoint: GatewayEndpoint,
    key: Option<SecretString>,
}

impl Gateway {
    /// A Gateway at a known address, with no key.
    #[must_use]
    pub fn new(endpoint: GatewayEndpoint) -> Gateway {
        Gateway { endpoint, key: None }
    }

    #[must_use]
    pub fn with_key(mut self, key: SecretString) -> Gateway {
        self.key = Some(key);
        self
    }

    #[must_use]
    pub fn endpoint(&self) -> &GatewayEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub fn key(&self) -> Option<&SecretString> {
        self.key.as_ref()
    }

    /// A client for this Gateway. It sends the key when there is one.
    #[must_use]
    pub fn client(&self) -> GatewayChat {
        match &self.key {
            Some(key) => GatewayChat::new(self.endpoint.clone(), key.clone()),
            None => GatewayChat::keyless(self.endpoint.clone()),
        }
    }

    /// Reads `RUNVIL_GATEWAY_URL` and `RUNVIL_GATEWAY_API_KEY`. The Host opts in by calling it.
    pub fn find_env() -> Result<Gateway, Error> {
        Gateway::find_env_named(DEFAULT_URL_VAR, DEFAULT_KEY_VAR)
    }

    /// The same read, with the variable names the Host chooses.
    pub fn find_env_named(url_var: &str, key_var: &str) -> Result<Gateway, Error> {
        from_lookup(url_var, key_var, |name| match std::env::var(name) {
            Ok(value) => Ok(Some(value)),
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => Err(make_invalid_env_error(name)),
        })
    }

    /// Finds a Gateway on this machine through `<home>/.runvil/run/gateway.json`.
    ///
    /// The call blocks for up to about two seconds while it proves the Gateway is live. A Host
    /// on an async runtime calls it from a blocking task.
    #[cfg(feature = "discovery")]
    pub fn find() -> Result<Gateway, Error> {
        let Some(run_dir) = crate::discovery::default_run_dir() else {
            return Err(Error::specific(
                ErrorKind::Unavailable,
                "the user's home directory could not be found",
            ));
        };
        Gateway::find_in(&run_dir)
    }

    /// `find` with the run directory named by the caller.
    #[cfg(feature = "discovery")]
    pub fn find_in(run_dir: &Path) -> Result<Gateway, Error> {
        let Some(file) = crate::discovery::read_live(run_dir)? else {
            return Err(Error::specific(
                ErrorKind::Unavailable,
                format!("no gateway is running: there is no gateway.json in {}", run_dir.display()),
            ));
        };
        crate::discovery::check_version(&file.version)?;
        let endpoint = GatewayEndpoint::new(&format!("{}/v1", file.local_addr()))?;
        let key = SecretString::new(file.api_key)?;
        Ok(Gateway::new(endpoint).with_key(key))
    }
}

/// The environment read behind `find_env_named`, with the lookup injected so tests need not touch
/// the process environment. The key is optional exactly when the URL's host is loopback.
fn from_lookup(
    url_var: &str,
    key_var: &str,
    lookup: impl Fn(&str) -> Result<Option<String>, Error>,
) -> Result<Gateway, Error> {
    let base_url = lookup(url_var)?.ok_or_else(|| make_missing_env_error(url_var))?;
    let endpoint = GatewayEndpoint::new(&base_url)?;
    let key = lookup(key_var)?.map(SecretString::new).and_then(Result::ok);
    match key {
        Some(key) => Ok(Gateway::new(endpoint).with_key(key)),
        None if endpoint.is_loopback() => Ok(Gateway::new(endpoint)),
        None => Err(make_missing_env_error(key_var)),
    }
}
```

The `/v1` suffix matches how Workshop builds its endpoint from a discovered base URL (gateway.rs): `format!("{}/v1", base_url.trim_end_matches('/'))`.

How a Host uses it, so the thread's questions have a concrete answer:

```rust
let gateway = Gateway::find()?;                       // the discovery file
let key = gateway.key().cloned().ok_or(no_key)?;      // a discovered Gateway always has one
let broker = GatewayBroker::new(gateway.endpoint().clone(), key.clone());
provide_gateway_resources(&mut resources, move || {
    Some((gateway.endpoint().clone(), key.clone()))   // each tool call reads a fresh clone
})?;
```

No `Rc<RefCell>` and no `Arc<Mutex>`: both consumers get clones, and the Plugin only ever sees the JSON entry.

### The contract fixture (in `runvil-gateway`, the writer unchanged)

```json
{
  "port": 52814,
  "api_key": "fixture-key",
  "pid": 4242,
  "epoch": 1700000000,
  "version": "0.1.0",
  "started_at": "2023-11-14T22:13:20Z"
}
```

```rust
// runvil-gateway/crates/gateway-api-discovery/tests/contract.rs   NEW
use gateway_api_discovery::GatewayDiscoveryFile;

const FIXTURE: &str = include_str!("fixtures/gateway.json");

/// What the writer puts on disk equals the fixture that readers pin.
#[test]
fn the_writer_output_matches_the_contract_fixture() {
    let dir = tempfile::tempdir().expect("a temp dir");
    let file = GatewayDiscoveryFile {
        port: 52814,
        api_key: "fixture-key".to_owned(),
        pid: 4242,
        epoch: 1_700_000_000,
        version: "0.1.0".to_owned(),
        started_at: "2023-11-14T22:13:20Z".to_owned(),
    };
    file.write_to(dir.path()).expect("the writer writes");
    let written = std::fs::read_to_string(dir.path().join("gateway.json")).expect("the file exists");
    let written: serde_json::Value = serde_json::from_str(&written).expect("the file is JSON");
    let fixture: serde_json::Value = serde_json::from_str(FIXTURE).expect("the fixture is JSON");
    assert_eq!(written, fixture);
}
```

The client keeps a pinned copy at `crates/gateway-client/tests/fixtures/gateway.json` and a unit test in `discovery/file.rs` that parses it with `include_str!` and asserts every field and `validation_error() == None`. A CI step downloads the fixture from the Gateway repository at a pinned commit and diffs it against the copy. This is a new check. The report says the client's tests check against the Gateway's pinned `openapi.json`, and I found no such check anywhere in `runvil-harness`, so treat that sentence in the report as unverified.

## Part 8. Audit list

What a reviewer checks, and how:

- **Nothing in the client names Runvil.** The client builds in a repository that holds no Runvil crate, so a dependency on one cannot resolve. Check that its manifests have no `path` or `git` entry for a Runvil crate. No guard test is needed.
- **The Harness's own checks still pass.** `cargo test -p build-xtask` runs unchanged, because `gateway-client` is not a workspace member and the product-boundary matrix does not reach it (product.rs).
- **Behavior is preserved.** The saved test list is accounted for name by name (Stage B step 8), and the test diff shows no changed literal (the porting rule).
- **No `non_exhaustive` and no `Internal` fallback.** Search the new repository for `non_exhaustive`, and the adapter for `ErrorCode::Internal`. Neither should appear.
- **Conversions are the adapter's own traits.** The compiler enforces this: a `From` impl between two foreign types does not compile.
- **The request bytes are the same.** `request-tests.rs` is ported with its JSON expectations unchanged. The adapter's broker tests send a real stepped run's messages to a mock gateway and read back the body. The system message and the assistant tool-call message have no public constructor, which is why the adapter keeps `runvil-engine` as a dev-dependency.
- **The error text is the same.** The client's classifier tests keep their expected strings. The adapter adds tests that run a round against the mock gateway for these cases and assert the Engine `Error`: a 401 (`Unavailable`, the credentials phrase), a 429 with quota text (`QuotaExhausted`), a context overflow with token counts (`Count` and `Limit` facts), a stream without `[DONE]` (`MalformedResponse`), an empty reply with a finish reason (`FinishReason` fact), and a missing environment variable (`Reason` and `Name` facts).
- **The phrase table has not drifted.** `each_model_kind_has_the_engine_phrase` compares all 12 phrases with the Engine's, visiting every kind through `ErrorKind::ALL`.
- **The error mapping keeps every fact.** `a_context_overflow_keeps_its_counts_as_engine_facts`, `a_setup_error_keeps_its_reason_and_name_as_engine_facts`, and `an_empty_reply_keeps_its_finish_reason_as_an_engine_fact`.
- **Every public item is documented.** `missing_docs` and `cargo doc` with `-D warnings` fail the build otherwise.
- **The two message types write the same JSON.** `engine_and_client_messages_write_the_same_json`.
- **The key is read in two places.** Search for `expose_secret(` in both crates. The client's `transport.rs` and `models.rs` (the `Authorization` header) and the adapter's `resources.rs` (the Plugin entry) are the only callers. The adapter's `models()` passes `&SecretString`, so it does not read the key.
- **The `unsafe` is the same.** Stage C moves 255 lines and 7 `unsafe` blocks. Diff `sys/` against the Gateway's copy, which must show only paths.
