//! Sends one question to the server that `RUNVIL_GATEWAY_URL` names, prints
//! the answer as it streams in, and prints the token counts.
//!
//! Set `RUNVIL_GATEWAY_URL` to the server's API root, for example
//! `http://127.0.0.1:8081/v1`, and `RUNVIL_GATEWAY_API_KEY` to its key when it
//! needs one. Pass the model name as the first argument.

use gateway_client::{Gateway, Message, Options, Outcome, Role, StreamDelta};

#[tokio::main]
async fn main() -> Result<(), gateway_client::Error> {
    let model = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "default".to_owned());
    let client = Gateway::find_env()?.client();
    let reply = client
        .complete(
            &[Message::text(Role::User, "Why is the sky blue?")],
            None,
            &Options::new(model),
            |delta| {
                if let StreamDelta::Text(text) = delta {
                    print!("{text}");
                }
            },
        )
        .await?;
    println!();
    match reply.outcome() {
        Outcome::Text(_) => {}
        Outcome::ToolCalls(calls) => println!("the model asked for {} tool calls", calls.len()),
    }
    if let Some(usage) = reply.usage() {
        println!(
            "{} prompt tokens, {} reply tokens",
            usage.prompt_tokens, usage.completion_tokens
        );
    }
    Ok(())
}
