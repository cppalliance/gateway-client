# gateway-client

A Rust client for chat-completions servers. It sends a conversation to a server, reads the streamed reply, and lists the server's models. It works against any server that speaks the OpenAI chat-completions protocol, and it knows the few extras that the [Runvil Gateway](https://github.com/cppalliance/runvil-gateway) adds.

The crate is not on crates.io. Depend on it by Git commit:

```toml
gateway-client = { version = "0.1.0", git = "https://github.com/cppalliance/gateway-client", rev = "<commit>" }
```

## Example

```rust
use gateway_client::{GatewayChat, GatewayEndpoint, Message, Options, Outcome, Role, SecretString};

let client = GatewayChat::new(
    GatewayEndpoint::new("http://127.0.0.1:8081/v1")?,
    SecretString::new("the-servers-key")?,
);
let messages = [Message::text(Role::User, "Why is the sky blue?")];
let reply = client
    .complete(&messages, None, &Options::new("qwen3-30b"), |_piece| {})
    .await?;
if let Outcome::Text(answer) = reply.outcome() {
    println!("{answer}");
}
```

`complete` takes its messages as any type that serde writes as a chat-completions message, so a program with its own message type passes it as it is. The callback gets each piece of the reply as it streams in. A runnable version is in [examples/chat.rs](crates/gateway-client/examples/chat.rs).

## License

Boost Software License 1.0. See [LICENSE](LICENSE).
