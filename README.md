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

## Finding a Gateway

A `Gateway` says where a Gateway is and how to log in. A program that knows the address builds one with `Gateway::new` and `with_key`. There are two ways to find one without an address:

- `Gateway::find()` reads `<home>/.runvil/run/gateway.json`, the file that a Gateway on this machine writes after it binds. It proves the Gateway is live before it trusts the file, and it never deletes the file. The call blocks for up to about two seconds, so a program on an async runtime calls it from a blocking task. The `discovery` feature gives it, and the feature is on by default.
- `Gateway::find_env()` reads `RUNVIL_GATEWAY_URL` and `RUNVIL_GATEWAY_API_KEY`, and `Gateway::find_env_named(url_var, key_var)` reads the names you choose. The key is optional for a loopback URL and required for any other host.

```rust
use gateway_client::Gateway;

let gateway = Gateway::find().or_else(|_| Gateway::find_env())?;
let client = gateway.client();
```

The Gateway writes the file, and the program that launches a Gateway starts and stops it. This crate only reads the file. A program that talks to a stock server and wants no platform code turns the feature off with `default-features = false`.

`complete` takes its messages as any type that serde writes as a chat-completions message, so a program with its own message type passes it as it is. The callback gets each piece of the reply as it streams in. A runnable version is in [examples/chat.rs](crates/gateway-client/examples/chat.rs).

## License

Boost Software License 1.0. See [LICENSE](LICENSE).
