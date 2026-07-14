use std::fmt;
use std::str::FromStr;

use serde_json::Value;

pub use crate::mcp::transport_http::HttpTransport;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
    Developer,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageRole::System => write!(f, "system"),
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::Tool => write!(f, "tool"),
            MessageRole::Developer => write!(f, "developer"),
        }
    }
}

impl FromStr for MessageRole {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "system" => Ok(MessageRole::System),
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "tool" => Ok(MessageRole::Tool),
            "developer" => Ok(MessageRole::Developer),
            _ => Err(format!("invalid message role: {}", s)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: String,
    pub name: String,
}

impl Message {
    pub fn to_json_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("role".to_string(), Value::String(self.role.to_string()));

        if self.role == MessageRole::Assistant && !self.tool_calls.is_empty() {
            map.insert("content".to_string(), Value::Null);
            let tc_arr: Vec<Value> = self
                .tool_calls
                .iter()
                .map(|tc| {
                    serde_json::json!({
                        "id": tc.id,
                        "type": "function",
                        "function": {
                            "name": tc.function_name,
                            "arguments": tc.arguments
                        }
                    })
                })
                .collect();
            map.insert("tool_calls".to_string(), Value::Array(tc_arr));
        } else {
            map.insert("content".to_string(), Value::String(self.content.clone()));
        }

        if self.role == MessageRole::Tool {
            map.insert(
                "tool_call_id".to_string(),
                Value::String(self.tool_call_id.clone()),
            );
            if !self.name.is_empty() {
                map.insert("name".to_string(), Value::String(self.name.clone()));
            }
        }

        Value::Object(map)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub function_name: String,
    pub arguments: String,
    pub tc_index: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiStreamChunk {
    Text(String),
    Reasoning(String),
    Usage {
        input_tokens: i32,
        output_tokens: i32,
    },
    ToolCall(ToolCall),
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ApiUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cache_read_tokens: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: ApiUsage,
    pub error: ApiError,
    pub finish_reason: String,
}

impl Default for ApiResponse {
    fn default() -> Self {
        ApiResponse {
            content: String::new(),
            tool_calls: Vec::new(),
            usage: ApiUsage::default(),
            error: ApiError {
                code: 0,
                message: String::new(),
            },
            finish_reason: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: i32,
    pub stream_options: Option<Value>,
}

pub struct ApiClient {
    pub config: ApiClientConfig,
    pub http: HttpTransport,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_role_display() {
        assert_eq!(MessageRole::System.to_string(), "system");
        assert_eq!(MessageRole::User.to_string(), "user");
        assert_eq!(MessageRole::Assistant.to_string(), "assistant");
        assert_eq!(MessageRole::Tool.to_string(), "tool");
        assert_eq!(MessageRole::Developer.to_string(), "developer");
    }

    #[test]
    fn test_message_role_from_str() {
        assert_eq!(
            "system".parse::<MessageRole>().unwrap(),
            MessageRole::System
        );
        assert_eq!("user".parse::<MessageRole>().unwrap(), MessageRole::User);
        assert_eq!(
            "assistant".parse::<MessageRole>().unwrap(),
            MessageRole::Assistant
        );
        assert_eq!("tool".parse::<MessageRole>().unwrap(), MessageRole::Tool);
        assert_eq!(
            "developer".parse::<MessageRole>().unwrap(),
            MessageRole::Developer
        );
        assert!("invalid".parse::<MessageRole>().is_err());
    }

    #[test]
    fn test_simple_user_message() {
        let msg = Message {
            role: MessageRole::User,
            content: "Hello".to_string(),
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        };
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
        assert!(msg.tool_calls.is_empty());
        assert!(msg.tool_call_id.is_empty());
    }

    #[test]
    fn test_assistant_message_with_tool_calls() {
        let tc = ToolCall {
            id: "call_123".to_string(),
            function_name: "read_file".to_string(),
            arguments: "{}".to_string(),
            tc_index: 0,
        };
        let msg = Message {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_calls: vec![tc],
            tool_call_id: String::new(),
            name: String::new(),
        };
        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].id, "call_123");
        assert_eq!(msg.tool_calls[0].function_name, "read_file");
    }

    #[test]
    fn test_tool_result_message() {
        let msg = Message {
            role: MessageRole::Tool,
            content: "file contents".to_string(),
            tool_calls: Vec::new(),
            tool_call_id: "call_123".to_string(),
            name: "read_file".to_string(),
        };
        assert_eq!(msg.role, MessageRole::Tool);
        assert_eq!(msg.tool_call_id, "call_123");
        assert_eq!(msg.name, "read_file");
    }

    #[test]
    fn test_developer_message() {
        let msg = Message {
            role: MessageRole::Developer,
            content: "reasoning instructions".to_string(),
            tool_calls: Vec::new(),
            tool_call_id: String::new(),
            name: String::new(),
        };
        assert_eq!(msg.role, MessageRole::Developer);
        assert_eq!(msg.content, "reasoning instructions");
    }

    #[test]
    fn test_basic_tool_with_parameters() {
        let params = serde_json::json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]});
        let tool = Tool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: params,
        };
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.description, "Read a file");
        assert_eq!(tool.parameters["type"].as_str(), Some("object"));
        assert_eq!(tool.parameters["required"][0].as_str(), Some("path"));
    }

    #[test]
    fn test_tool_without_description() {
        let params = serde_json::json!({"type": "object"});
        let tool = Tool {
            name: "echo".to_string(),
            description: String::new(),
            parameters: params,
        };
        assert_eq!(tool.name, "echo");
        assert!(tool.description.is_empty());
    }

    #[test]
    fn test_complete_tool_call() {
        let tc = ToolCall {
            id: "call_456".to_string(),
            function_name: "write_file".to_string(),
            arguments: "{\"path\":\"test.txt\"}".to_string(),
            tc_index: 0,
        };
        assert_eq!(tc.id, "call_456");
        assert_eq!(tc.function_name, "write_file");
        assert_eq!(tc.arguments, "{\"path\":\"test.txt\"}");
    }

    #[test]
    fn test_partial_tool_call() {
        let tc = ToolCall {
            id: String::new(),
            function_name: String::new(),
            arguments: String::new(),
            tc_index: 0,
        };
        assert!(tc.id.is_empty());
        assert!(tc.function_name.is_empty());
        assert!(tc.arguments.is_empty());
    }

    #[test]
    fn test_api_stream_chunk_text() {
        let c = ApiStreamChunk::Text("Hello World".to_string());
        if let ApiStreamChunk::Text(t) = &c {
            assert_eq!(t, "Hello World");
        } else {
            panic!("expected Text");
        }
    }

    #[test]
    fn test_api_stream_chunk_reasoning() {
        let c = ApiStreamChunk::Reasoning("thinking step".to_string());
        if let ApiStreamChunk::Reasoning(r) = &c {
            assert_eq!(r, "thinking step");
        } else {
            panic!("expected Reasoning");
        }
    }

    #[test]
    fn test_api_stream_chunk_usage() {
        let c = ApiStreamChunk::Usage {
            input_tokens: 10,
            output_tokens: 5,
        };
        if let ApiStreamChunk::Usage {
            input_tokens,
            output_tokens,
        } = &c
        {
            assert_eq!(*input_tokens, 10);
            assert_eq!(*output_tokens, 5);
        } else {
            panic!("expected Usage");
        }
    }

    #[test]
    fn test_api_stream_chunk_tool_call() {
        let tc = ToolCall {
            id: "call_1".to_string(),
            function_name: "read_file".to_string(),
            arguments: "{}".to_string(),
            tc_index: 0,
        };
        let c = ApiStreamChunk::ToolCall(tc);
        if let ApiStreamChunk::ToolCall(t) = &c {
            assert_eq!(t.id, "call_1");
            assert_eq!(t.function_name, "read_file");
        } else {
            panic!("expected ToolCall");
        }
    }

    #[test]
    fn test_api_stream_chunk_done() {
        let c = ApiStreamChunk::Done;
        assert!(matches!(c, ApiStreamChunk::Done));
    }

    #[test]
    fn test_api_response_defaults() {
        let resp = ApiResponse::default();
        assert!(resp.content.is_empty());
        assert!(resp.tool_calls.is_empty());
        assert!(resp.finish_reason.is_empty());
        assert_eq!(resp.error.code, 0);
    }

    #[test]
    fn test_api_response_with_content_and_usage() {
        let usage = ApiUsage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        };
        let resp = ApiResponse {
            content: "Hello".to_string(),
            usage,
            finish_reason: "stop".to_string(),
            ..Default::default()
        };
        assert_eq!(resp.content, "Hello");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.finish_reason, "stop");
    }

    #[test]
    fn test_api_response_with_error() {
        let err = ApiError {
            code: 401,
            message: "invalid API key".to_string(),
        };
        let resp = ApiResponse {
            error: err,
            ..Default::default()
        };
        assert_eq!(resp.error.code, 401);
        assert_eq!(resp.error.message, "invalid API key");
    }

    #[test]
    fn test_minimal_config() {
        let cfg = ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1".to_string(),
            api_key: "sk-xxx".to_string(),
            model: "model-id".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        };
        assert_eq!(cfg.base_url, "https://openrouter.ai/api/v1");
        assert_eq!(cfg.api_key, "sk-xxx");
        assert_eq!(cfg.model, "model-id");
        assert_eq!(cfg.temperature, 0.0);
        assert_eq!(cfg.max_tokens, 0);
    }

    #[test]
    fn test_full_config() {
        let cfg = ApiClientConfig {
            base_url: "http://localhost:11434/v1".to_string(),
            api_key: String::new(),
            model: "llama3".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            stream_options: Some(serde_json::json!({"include_usage": true})),
        };
        assert_eq!(cfg.base_url, "http://localhost:11434/v1");
        assert_eq!(cfg.model, "llama3");
        assert_eq!(cfg.temperature, 0.7);
        assert_eq!(cfg.max_tokens, 2048);
        assert_eq!(
            cfg.stream_options.as_ref().unwrap()["include_usage"].as_bool(),
            Some(true)
        );
    }
}
