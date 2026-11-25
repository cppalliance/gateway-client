# gateway-client

A Rust client for chat-completions servers. It sends a conversation to a server, reads the streamed reply, and lists the server's models. It works against any server that speaks the OpenAI chat-completions protocol, and it knows the few extras that the [Runvil Gateway](https://github.com/cppalliance/runvil-gateway) adds.

The crate is not on crates.io. Depend on it by Git commit:

```toml
gateway-client = { version = "0.1.0", git = "https://github.com/cppalliance/gateway-client", rev = "<commit>" }
```

## License

Boost Software License 1.0. See [LICENSE](LICENSE).
