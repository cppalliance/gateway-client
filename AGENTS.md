# gateway-client

A Rust client for chat-completions servers. It sends a conversation to a server, reads the streamed reply, and lists the server's models. It works against any server that speaks the OpenAI chat-completions protocol, and it knows the few extras that the Runvil Gateway adds. A program that uses it does not need Runvil.

The Gateway is Runvil's own server, in `cppalliance/runvil-gateway`. It forwards each model request to a vendor or to a local model.

## Principles

- Design for human review. A reviewer reads the crate's code and its surface: every item another crate can name, methods, fields, and variants included. Keep both small.
- Do more with less. Before building, check whether something that exists does the work.
- Error and status messages assume a model reads them: concise, factual, and self-contained, naming what is missing or unmet with required versus actual.
- There are no users and no stored data to preserve, so backward compatibility does not matter: change APIs freely, and never bump a crate version number.

## Engineering

- No `#[non_exhaustive]`. A wildcard arm in another crate turns a new variant into a run-time case. A crate that matches on `ErrorKind`, `Outcome`, `Thinking`, or `StreamDelta` must fail to compile when one is added.
- The crate reports every failure as `Error` with an `ErrorKind`. `Error.summary` holds this crate's own wording and never server text. Server text goes in `Error.message` only.
- `build_request_body` is the only place the chat-completions body is shaped, so every request for one conversation sends the same JSON. Never build or patch a body elsewhere.
- A message goes to the server as whatever `serde::Serialize` writes. The caller's type decides the JSON, and the client checks only that it is an array of objects that each have a string `role`.
- `serde_json` keeps `float_roundtrip`, and `preserve_order` stays off.
- `unsafe` code stays in `discovery/sys`, where each block has its own `#[expect(unsafe_code, reason = ...)]` and a safety comment.
- The discovery reader only reads. It never writes, launches, stops, or deletes `gateway.json`, because the Gateway and the program that launches it own those jobs. `tests/fixtures/gateway.json` is a pinned copy of the Gateway's contract fixture, and the `contract` CI job diffs it against the Gateway's copy at the commit that the job names.
- Pinned by Git commit. The crate is not published on crates.io.
- A Cargo feature gates a real constraint, such as platform code that needs extra dependencies, and not product shape.
- Comments explain a non-obvious constraint, ordering requirement, or workaround.

## Verification

- Tests: `cargo nextest run --locked --workspace --all-features`.
- Lint: `cargo clippy --workspace --all-targets --all-features`, with `CARGO_BUILD_WARNINGS=deny`. Clippy covers `cargo check`, so never run a standalone `cargo check --workspace`.
- Format: `cargo fmt --all --check`.
- Docs: `cargo doc --workspace --no-deps --all-features`, with `RUSTDOCFLAGS="-D warnings"`.
- Supply chain: `cargo deny check` and `cargo audit`.
- Custom lints: `cargo dylint --all -- --locked --workspace --all-targets --all-features`, with `cargo-dylint` and `dylint-link` installed. The lints live in `cppalliance/dylints`, at the `rev` the root `Cargo.toml` pins.

## Structural rules

- Production code uses the standard module layout: `foo.rs`, with `foo/bar.rs` for its children. No `#[path]` in production code.
- Test code goes at the bottom of the file under test in a `#[cfg(test)] mod tests { }` block when practical, and otherwise in `foo-tests.rs` beside it, wired with `#[path = "foo-tests.rs"] mod tests;`. These are the only files wired with `#[path]`.
- A file holds one concept and has no length limit. Size limits sit on functions: `clippy.toml` sets 100 lines.
