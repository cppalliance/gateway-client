//! The chat message a caller without its own message type can build.
//!
//! A round takes any type that serde can write as the chat-completions
//! message JSON, so a caller with its own type passes it as it is. [`Message`]
//! is for the caller that has none.

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
    /// A text message from `role`. A tool result also names the call it
    /// answers, so it has its own constructor, [`Message::tool`].
    #[must_use]
    pub fn text(role: Role, content: impl Into<String>) -> Message {
        Message {
            role,
            content: Value::String(content.into()),
            tool_call_id: None,
        }
    }

    /// A `tool` message holding the result of the call named by
    /// `tool_call_id`.
    #[must_use]
    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Message {
        Message {
            role: Role::Tool,
            content: Value::String(content.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_text_message_writes_only_role_and_content() {
        let value = serde_json::to_value(Message::text(Role::User, "hi")).expect("serializes");
        assert_eq!(value, json!({ "role": "user", "content": "hi" }));
    }

    #[test]
    fn each_role_writes_its_lowercase_name() {
        for (role, name) in [
            (Role::System, "system"),
            (Role::User, "user"),
            (Role::Assistant, "assistant"),
            (Role::Tool, "tool"),
        ] {
            let value = serde_json::to_value(Message::text(role, "x")).expect("serializes");
            assert_eq!(value["role"], name);
        }
    }

    #[test]
    fn a_tool_message_names_the_call_it_answers() {
        let value = serde_json::to_value(Message::tool("call_1", "42")).expect("serializes");
        assert_eq!(
            value,
            json!({ "role": "tool", "content": "42", "tool_call_id": "call_1" })
        );
    }
}
