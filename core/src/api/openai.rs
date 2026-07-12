use std::collections::HashMap;

use serde_json::Value;

use crate::api::types::*;
use crate::mcp::sse::SseEvent;

pub fn new_client(config: ApiClientConfig) -> ApiClient {
    let endpoint_url = if config.base_url.ends_with("/chat/completions") {
        config.base_url.clone()
    } else {
        format!("{}/chat/completions", config.base_url)
    };
    let http = HttpTransport::new(&endpoint_url, &config.api_key);
    ApiClient { config, http }
}

pub fn build_chat_request(client: &ApiClient, messages: &[Message], tools: &[Tool]) -> Value {
    let msg_arr: Vec<Value> = messages.iter().map(|m| m.to_json_value()).collect();

    let mut map = serde_json::Map::new();
    map.insert("model".to_string(), Value::String(client.config.model.clone()));
    map.insert("messages".to_string(), Value::Array(msg_arr));
    map.insert("stream".to_string(), Value::Bool(false));

    if client.config.temperature > 0.0 {
        map.insert(
            "temperature".to_string(),
            Value::from(client.config.temperature),
        );
    }
    if client.config.max_tokens > 0 {
        map.insert(
            "max_tokens".to_string(),
            Value::from(client.config.max_tokens),
        );
    }
    if !tools.is_empty() {
        let tool_arr: Vec<Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters
                    }
                })
            })
            .collect();
        map.insert("tools".to_string(), Value::Array(tool_arr));
    }

    Value::Object(map)
}

pub fn parse_chat_response(body: &str) -> ApiResponse {
    let root: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => {
            return ApiResponse {
                error: ApiError {
                    code: 0,
                    message: "invalid JSON response".to_string(),
                },
                ..Default::default()
            };
        }
    };

    if let Some(err) = root.get("error") {
        let code = err.get("code").and_then(|c| c.as_i64()).unwrap_or(0) as i32;
        let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("");
        return ApiResponse {
            error: ApiError {
                code,
                message: msg.to_string(),
            },
            ..Default::default()
        };
    }

    let mut usage = ApiUsage::default();
    if let Some(u) = root.get("usage") {
        if let Some(v) = u.get("prompt_tokens").and_then(|v| v.as_i64()) {
            usage.input_tokens = v as i32;
        }
        if let Some(v) = u.get("completion_tokens").and_then(|v| v.as_i64()) {
            usage.output_tokens = v as i32;
        }
        if let Some(v) = u.get("cache_read_tokens").and_then(|v| v.as_i64()) {
            usage.cache_read_tokens = v as i32;
        }
    }

    let choices = match root.get("choices").and_then(|c| c.as_array()) {
        Some(c) if !c.is_empty() => c,
        _ => {
            if usage.input_tokens > 0 || usage.output_tokens > 0 {
                return ApiResponse {
                    usage,
                    ..Default::default()
                };
            }
            return ApiResponse {
                error: ApiError {
                    code: 0,
                    message: "no choices in response".to_string(),
                },
                usage,
                ..Default::default()
            };
        }
    };

    let choice = &choices[0];
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("")
        .to_string();

    let mut content = String::new();
    let mut tool_calls = Vec::new();

    if let Some(msg) = choice.get("message") {
        if let Some(c) = msg.get("content")
            && !c.is_null() {
                content = c.as_str().unwrap_or("").to_string();
            }
        if let Some(tcs) = msg.get("tool_calls").and_then(|t| t.as_array()) {
            for tc in tcs {
                let fn_obj = tc.get("function");
                let tc_id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                let fn_name = fn_obj
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let fn_args = fn_obj
                    .and_then(|f| f.get("arguments"))
                    .and_then(|a| a.as_str())
                    .unwrap_or("");
                tool_calls.push(ToolCall {
                    id: tc_id.to_string(),
                    function_name: fn_name.to_string(),
                    arguments: fn_args.to_string(),
                    tc_index: 0,
                });
            }
        }
    }

    ApiResponse {
        content,
        tool_calls,
        usage,
        finish_reason,
        ..Default::default()
    }
}

pub async fn create_message(client: &mut ApiClient, messages: &[Message], tools: &[Tool]) -> ApiResponse {
    let req_body = build_chat_request(client, messages, tools);
    let http_resp = client.http.post_json(&req_body.to_string()).await;
    if http_resp.status_code == 0 {
        return ApiResponse {
            error: ApiError {
                code: 0,
                message: http_resp.error,
            },
            ..Default::default()
        };
    }
    if http_resp.status_code != 200 {
        return ApiResponse {
            error: ApiError {
                code: http_resp.status_code,
                message: http_resp.body,
            },
            ..Default::default()
        };
    }
    parse_chat_response(&http_resp.body)
}

pub fn parse_stream_event(data: &str) -> Vec<ApiStreamChunk> {
    if data == "[DONE]" {
        return vec![ApiStreamChunk::Done];
    }

    let root: Value = match serde_json::from_str(data) {
        Ok(v) => v,
        Err(_) => {
            return vec![ApiStreamChunk::Text(String::new())];
        }
    };

    if root.get("error").is_some() {
        return vec![ApiStreamChunk::Text(String::new())];
    }

    if let Some(u) = root.get("usage") {
        let input = u
            .get("prompt_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        let output = u
            .get("completion_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0) as i32;
        return vec![ApiStreamChunk::Usage {
            input_tokens: input,
            output_tokens: output,
        }];
    }

    if let Some(choices) = root.get("choices").and_then(|c| c.as_array())
        && !choices.is_empty() {
            let choice = &choices[0];
            let delta = match choice.get("delta") {
                Some(d) => d,
                None => return vec![ApiStreamChunk::Text(String::new())],
            };

            if let Some(rc) = delta.get("reasoning_content")
                && !rc.is_null()
                    && let Some(s) = rc.as_str() {
                        return vec![ApiStreamChunk::Reasoning(s.to_string())];
                    }

            if let Some(tc_arr) = delta.get("tool_calls").and_then(|t| t.as_array())
                && !tc_arr.is_empty() {
                    let mut chunks = Vec::new();
                    for tc_data in tc_arr {
                        let mut tc = ToolCall {
                            id: String::new(),
                            function_name: String::new(),
                            arguments: String::new(),
                            tc_index: tc_data
                                .get("index")
                                .and_then(|i| i.as_i64())
                                .unwrap_or(0) as i32,
                        };
                        if let Some(id_val) = tc_data.get("id").and_then(|i| i.as_str()) {
                            tc.id = id_val.to_string();
                        }
                        if let Some(fn_obj) = tc_data.get("function") {
                            if let Some(name_val) = fn_obj.get("name").and_then(|n| n.as_str()) {
                                tc.function_name = name_val.to_string();
                            }
                            if let Some(arg_val) = fn_obj.get("arguments").and_then(|a| a.as_str())
                            {
                                tc.arguments = arg_val.to_string();
                            }
                        }
                        chunks.push(ApiStreamChunk::ToolCall(tc));
                    }
                    return chunks;
                }

            if let Some(content_val) = delta.get("content")
                && !content_val.is_null()
                    && let Some(s) = content_val.as_str() {
                        return vec![ApiStreamChunk::Text(s.to_string())];
                    }

            return vec![ApiStreamChunk::Text(String::new())];
        }

    vec![ApiStreamChunk::Text(String::new())]
}

pub async fn create_message_stream(
    client: &mut ApiClient,
    messages: &[Message],
    tools: &[Tool],
    mut on_chunk: impl FnMut(ApiStreamChunk) -> bool,
) -> ApiResponse {
    let mut req_body = build_chat_request(client, messages, tools);
    if let Some(obj) = req_body.as_object_mut() {
        obj.insert("stream".to_string(), Value::Bool(true));
        let stream_opts = client
            .config
            .stream_options
            .clone()
            .unwrap_or(serde_json::json!({"include_usage": true}));
        obj.insert("stream_options".to_string(), stream_opts);
    }

    let mut tool_call_state: HashMap<i32, ToolCall> = HashMap::new();
    let mut accumulated_content = String::new();
    let mut usage = ApiUsage::default();

    {
        let mut on_event = |event: SseEvent| -> bool {
            let chunks = parse_stream_event(&event.data);
            for chunk in chunks {
                match chunk {
                    ApiStreamChunk::Text(ref t) => {
                        accumulated_content.push_str(t);
                    }
                    ApiStreamChunk::Reasoning(_) => {}
                    ApiStreamChunk::Usage {
                        input_tokens,
                        output_tokens,
                    } => {
                        if input_tokens > 0 {
                            usage.input_tokens = input_tokens;
                        }
                        if output_tokens > 0 {
                            usage.output_tokens = output_tokens;
                        }
                    }
                    ApiStreamChunk::ToolCall(ref tc) => {
                        let tc_index = tc.tc_index;
                        let state = tool_call_state
                            .entry(tc_index)
                            .or_insert_with(|| ToolCall {
                                id: String::new(),
                                function_name: String::new(),
                                arguments: String::new(),
                                tc_index,
                            });
                        if !tc.id.is_empty() {
                            state.id = tc.id.clone();
                        }
                        if !tc.function_name.is_empty() {
                            state.function_name = tc.function_name.clone();
                        }
                        if !tc.arguments.is_empty() {
                            state.arguments.push_str(&tc.arguments);
                        }
                        let emit_chunk = ApiStreamChunk::ToolCall(state.clone());
                        if !on_chunk(emit_chunk) {
                            return false;
                        }
                        continue;
                    }
                    ApiStreamChunk::Done => {
                        on_chunk(ApiStreamChunk::Done);
                        return false;
                    }
                }
                if !on_chunk(chunk) {
                    return false;
                }
            }
            true
        };

        let (status_code, err_msg) = client.http.post_json_stream(&req_body.to_string(), &mut on_event).await;
        if status_code != 200 {
            return ApiResponse {
                error: ApiError {
                    code: status_code,
                    message: err_msg,
                },
                ..Default::default()
            };
        }
    }

    let mut result = ApiResponse {
        content: accumulated_content,
        usage,
        ..Default::default()
    };

    if !tool_call_state.is_empty() {
        let max_idx = tool_call_state.keys().max().copied().unwrap_or(-1);
        let mut tcs = Vec::new();
        for idx in 0..=max_idx {
            if let Some(tc) = tool_call_state.remove(&idx) {
                tcs.push(tc);
            }
        }
        result.tool_calls = tcs;
    }

    result
}

#[cfg(test)]
mod tests {
    fn sse_chunk(data: &str) -> String {
        format!("data: {}\n\n", data)
    }
    use super::*;

    #[test]
    fn test_basic_request_structure() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "test-model".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let req = build_chat_request(
            &client,
            &[Message {
                role: MessageRole::User,
                content: "hi".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
        );
        assert_eq!(req["model"].as_str(), Some("test-model"));
        assert!(!req["stream"].as_bool().unwrap());
        assert_eq!(req["messages"].as_array().unwrap().len(), 1);
        assert_eq!(req["messages"][0]["role"].as_str(), Some("user"));
        assert_eq!(req["messages"][0]["content"].as_str(), Some("hi"));
    }

    #[test]
    fn test_multiple_messages() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "test-model".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let req = build_chat_request(
            &client,
            &[
                Message {
                    role: MessageRole::System,
                    content: "you are a bot".to_string(),
                    tool_calls: Vec::new(),
                    tool_call_id: String::new(),
                    name: String::new(),
                },
                Message {
                    role: MessageRole::User,
                    content: "hello".to_string(),
                    tool_calls: Vec::new(),
                    tool_call_id: String::new(),
                    name: String::new(),
                },
            ],
            &[],
        );
        assert_eq!(req["messages"].as_array().unwrap().len(), 2);
        assert_eq!(req["messages"][0]["role"].as_str(), Some("system"));
        assert_eq!(req["messages"][1]["role"].as_str(), Some("user"));
    }

    #[test]
    fn test_temperature_and_max_tokens_when_set() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.7,
            max_tokens: 2048,
            stream_options: None,
        });
        let req = build_chat_request(
            &client,
            &[Message {
                role: MessageRole::User,
                content: "hi".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
        );
        assert!((req["temperature"].as_f64().unwrap() - 0.7).abs() < 1e-10);
        assert_eq!(req["max_tokens"].as_i64().unwrap(), 2048);
    }

    #[test]
    fn test_no_temperature_when_zero() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let req = build_chat_request(
            &client,
            &[Message {
                role: MessageRole::User,
                content: "hi".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
        );
        assert!(req.get("temperature").is_none());
    }

    #[test]
    fn test_tools_included_when_non_empty() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let params = serde_json::json!({"type": "object"});
        let tools = vec![Tool {
            name: "fn1".to_string(),
            description: "desc".to_string(),
            parameters: params,
        }];
        let req = build_chat_request(
            &client,
            &[Message {
                role: MessageRole::User,
                content: "hi".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &tools,
        );
        assert!(req.get("tools").is_some());
        assert_eq!(req["tools"].as_array().unwrap().len(), 1);
        assert_eq!(req["tools"][0]["type"].as_str(), Some("function"));
        assert_eq!(
            req["tools"][0]["function"]["name"].as_str(),
            Some("fn1")
        );
        assert_eq!(
            req["tools"][0]["function"]["description"].as_str(),
            Some("desc")
        );
    }

    #[test]
    fn test_no_tools_field_when_empty() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let req = build_chat_request(
            &client,
            &[Message {
                role: MessageRole::User,
                content: "hi".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
        );
        assert!(req.get("tools").is_none());
    }

    #[test]
    fn test_assistant_with_tool_calls_has_null_content() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let tc = ToolCall {
            id: "call_1".to_string(),
            function_name: "read_file".to_string(),
            arguments: "{}".to_string(),
            tc_index: 0,
        };
        let msgs = vec![Message {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_calls: vec![tc],
            tool_call_id: String::new(),
            name: String::new(),
        }];
        let req = build_chat_request(&client, &msgs, &[]);
        let msg = &req["messages"][0];
        assert_eq!(msg["role"].as_str(), Some("assistant"));
        assert!(msg["content"].is_null());
        assert_eq!(msg["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(msg["tool_calls"][0]["id"].as_str(), Some("call_1"));
        assert_eq!(
            msg["tool_calls"][0]["type"].as_str(),
            Some("function")
        );
        assert_eq!(
            msg["tool_calls"][0]["function"]["name"].as_str(),
            Some("read_file")
        );
        assert_eq!(
            msg["tool_calls"][0]["function"]["arguments"].as_str(),
            Some("{}")
        );
    }

    #[test]
    fn test_tool_message_includes_tool_call_id() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: String::new(),
            model: "m".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        let msgs = vec![Message {
            role: MessageRole::Tool,
            content: "result".to_string(),
            tool_calls: Vec::new(),
            tool_call_id: "call_1".to_string(),
            name: "read_file".to_string(),
        }];
        let req = build_chat_request(&client, &msgs, &[]);
        let msg = &req["messages"][0];
        assert_eq!(msg["role"].as_str(), Some("tool"));
        assert_eq!(msg["content"].as_str(), Some("result"));
        assert_eq!(msg["tool_call_id"].as_str(), Some("call_1"));
        assert_eq!(msg["name"].as_str(), Some("read_file"));
    }

    #[test]
    fn test_parse_chat_response_normal() {
        let body = r#"{"id":"chat-1","choices":[{"index":0,"message":{"role":"assistant","content":"Hello!"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#;
        let resp = parse_chat_response(body);
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.finish_reason, "stop");
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 5);
        assert_eq!(resp.error.code, 0);
    }

    #[test]
    fn test_parse_chat_response_with_tool_calls() {
        let body = r#"{"id":"chat-2","choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"read_file","arguments":"{\"path\":\"test.txt\"}"}}]},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":10}}"#;
        let resp = parse_chat_response(body);
        assert!(resp.content.is_empty());
        assert_eq!(resp.finish_reason, "tool_calls");
        assert_eq!(resp.tool_calls.len(), 1);
        assert_eq!(resp.tool_calls[0].id, "call_1");
        assert_eq!(resp.tool_calls[0].function_name, "read_file");
        assert_eq!(resp.tool_calls[0].arguments, "{\"path\":\"test.txt\"}");
        assert_eq!(resp.usage.input_tokens, 20);
    }

    #[test]
    fn test_parse_chat_response_error_401() {
        let body = r#"{"error":{"code":401,"message":"Invalid API key"}}"#;
        let resp = parse_chat_response(body);
        assert_eq!(resp.error.code, 401);
        assert_eq!(resp.error.message, "Invalid API key");
    }

    #[test]
    fn test_parse_chat_response_error_400() {
        let body = r#"{"error":{"code":400,"message":"Bad request"}}"#;
        let resp = parse_chat_response(body);
        assert_eq!(resp.error.code, 400);
    }

    #[test]
    fn test_parse_chat_response_error_429() {
        let body = r#"{"error":{"code":429,"message":"Rate limit exceeded"}}"#;
        let resp = parse_chat_response(body);
        assert_eq!(resp.error.code, 429);
    }

    #[test]
    fn test_parse_chat_response_empty_choices() {
        let body = r#"{"id":"chat-3","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":0}}"#;
        let resp = parse_chat_response(body);
        assert_eq!(resp.error.code, 0);
        assert!(resp.content.is_empty());
        assert_eq!(resp.usage.input_tokens, 5);
    }

    #[test]
    fn test_parse_chat_response_invalid_json() {
        let resp = parse_chat_response("not json");
        assert_eq!(resp.error.code, 0);
        assert_eq!(resp.error.message, "invalid JSON response");
    }

    #[test]
    fn test_new_client_creates_client_with_config() {
        let client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key: "sk-test".to_string(),
            model: "llama3".to_string(),
            temperature: 0.0,
            max_tokens: 0,
            stream_options: None,
        });
        assert_eq!(
            client.config.base_url,
            "https://openrouter.ai/api/v1/chat/completions"
        );
        assert_eq!(client.config.api_key, "sk-test");
        assert_eq!(client.config.model, "llama3");
    }

    #[test]
    fn test_parse_stream_event_plain_text_delta() {
        let data = r#"{"id":"1","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Text(t) if t == "Hello"));
    }

    #[test]
    fn test_parse_stream_event_text_delta_with_null_content() {
        let data = r#"{"id":"1","choices":[{"index":0,"delta":{"content":null},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Text(t) if t.is_empty()));
    }

    #[test]
    fn test_parse_stream_event_tool_call_delta_with_id_and_name() {
        let data = r#"{"id":"gen-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::ToolCall(tc) if tc.id == "call_1" && tc.function_name == "read_file"));
    }

    #[test]
    fn test_parse_stream_event_tool_call_delta_with_arguments_only() {
        let data = r#"{"id":"gen-1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\": \"test.txt\"}"}}]},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::ToolCall(tc) if tc.tc_index == 0 && tc.arguments == "{\"path\": \"test.txt\"}"));
    }

    #[test]
    fn test_parse_stream_event_reasoning_content_delta() {
        let data = r#"{"id":"1","choices":[{"index":0,"delta":{"reasoning_content":"thinking step"},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Reasoning(r) if r == "thinking step"));
    }

    #[test]
    fn test_parse_stream_event_usage_chunk() {
        let data = r#"{"usage":{"prompt_tokens":10,"completion_tokens":5}}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Usage { input_tokens: 10, output_tokens: 5 }));
    }

    #[test]
    fn test_parse_stream_event_usage_chunk_partial() {
        let data = r#"{"usage":{"prompt_tokens":10}}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Usage { input_tokens: 10, output_tokens: 0 }));
    }

    #[test]
    fn test_parse_stream_event_done_marker() {
        let chunks = parse_stream_event("[DONE]");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(chunks[0], ApiStreamChunk::Done));
    }

    #[test]
    fn test_parse_stream_event_error_response() {
        let data = r#"{"error":{"code":401,"message":"Invalid API key"}}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Text(t) if t.is_empty()));
    }

    #[test]
    fn test_parse_stream_event_empty_delta_with_only_finish_reason() {
        let data = r#"{"id":"1","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Text(t) if t.is_empty()));
    }

    #[test]
    fn test_parse_stream_event_multiple_tool_calls() {
        let data = r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"fn1","arguments":""}},{"index":1,"id":"call_2","function":{"name":"fn2","arguments":""}}]},"finish_reason":null}]}"#;
        let chunks = parse_stream_event(data);
        assert_eq!(chunks.len(), 2);
        assert!(matches!(&chunks[0], ApiStreamChunk::ToolCall(tc) if tc.function_name == "fn1"));
        assert!(matches!(&chunks[1], ApiStreamChunk::ToolCall(tc) if tc.function_name == "fn2"));
    }

    #[test]
    fn test_parse_stream_event_json_parse_error_returns_empty_text() {
        let chunks = parse_stream_event("not valid json at all");
        assert_eq!(chunks.len(), 1);
        assert!(matches!(&chunks[0], ApiStreamChunk::Text(t) if t.is_empty()));
    }

    #[test]
    fn test_tool_call_delta_accumulation_single_tool_cross_chunk() {
        let sse = sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"#) +
                  &sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\": "}}]},"finish_reason":null}]}"#) +
                  &sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"test.txt\"}"}}]},"finish_reason":null}]}"#);
        let mut parser = crate::mcp::sse::SseParser::new();
        let mut tool_calls: HashMap<i32, ToolCall> = HashMap::new();
        for evt in parser.feed(&sse) {
            for chunk in parse_stream_event(&evt.data) {
                if let ApiStreamChunk::ToolCall(tc) = chunk {
                    let idx = tc.tc_index;
                    let state = tool_calls.entry(idx).or_insert_with(|| ToolCall {
                        id: String::new(),
                        function_name: String::new(),
                        arguments: String::new(),
                        tc_index: idx,
                    });
                    if !tc.id.is_empty() {
                        state.id = tc.id;
                    }
                    if !tc.function_name.is_empty() {
                        state.function_name = tc.function_name;
                    }
                    if !tc.arguments.is_empty() {
                        state.arguments.push_str(&tc.arguments);
                    }
                }
            }
        }
        assert_eq!(tool_calls.len(), 1);
        let tc = tool_calls.get(&0).unwrap();
        assert_eq!(tc.id, "call_1");
        assert_eq!(tc.function_name, "read_file");
        assert_eq!(tc.arguments, "{\"path\": \"test.txt\"}");
    }

    #[test]
    fn test_tool_call_delta_accumulation_multi_tool_parallel() {
        let sse = sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"fn1","arguments":""}},{"index":1,"id":"call_2","type":"function","function":{"name":"fn2","arguments":""}}]},"finish_reason":null}]}"#) +
                  &sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"a\":1}"}}]},"finish_reason":null}]}"#) +
                  &sse_chunk(r#"{"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":1,"function":{"arguments":"{\"b\":2}"}}]},"finish_reason":null}]}"#);
        let mut parser = crate::mcp::sse::SseParser::new();
        let mut tool_calls: HashMap<i32, ToolCall> = HashMap::new();
        for evt in parser.feed(&sse) {
            for chunk in parse_stream_event(&evt.data) {
                if let ApiStreamChunk::ToolCall(tc) = chunk {
                    let idx = tc.tc_index;
                    let state = tool_calls.entry(idx).or_insert_with(|| ToolCall {
                        id: String::new(),
                        function_name: String::new(),
                        arguments: String::new(),
                        tc_index: idx,
                    });
                    if !tc.id.is_empty() {
                        state.id = tc.id;
                    }
                    if !tc.function_name.is_empty() {
                        state.function_name = tc.function_name;
                    }
                    if !tc.arguments.is_empty() {
                        state.arguments.push_str(&tc.arguments);
                    }
                }
            }
        }
        assert_eq!(tool_calls.len(), 2);
        let tc0 = tool_calls.get(&0).unwrap();
        assert_eq!(tc0.id, "call_1");
        assert_eq!(tc0.function_name, "fn1");
        assert_eq!(tc0.arguments, "{\"a\":1}");
        let tc1 = tool_calls.get(&1).unwrap();
        assert_eq!(tc1.id, "call_2");
        assert_eq!(tc1.function_name, "fn2");
        assert_eq!(tc1.arguments, "{\"b\":2}");
    }

    #[test]
    fn test_sse_parser_ignores_comments() {
        let sse_text = ": OPENROUTER PROCESSING\n\ndata: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n";
        let mut parser = crate::mcp::sse::SseParser::new();
        let events = parser.feed(sse_text);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].data,
            r#"{"id":"1","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}"#
        );
    }

    #[tokio::test]
    async fn test_real_api_non_streaming_text() {
        let api_key = match get_openrouter_api_key() {
            Some(key) if !key.is_empty() => key,
            _ => {
                eprintln!("OPENROUTER_API_KEY not found in ~/.bashrc, skipping");
                return;
            }
        };
        let mut client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key,
            model: "meta-llama/llama-3.1-8b-instruct".to_string(),
            temperature: 0.0,
            max_tokens: 20,
            stream_options: None,
        });
        let resp = create_message(
            &mut client,
            &[Message {
                role: MessageRole::User,
                content: "Say hello in one word".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
        )
        .await;
        assert_eq!(resp.error.code, 0);
        assert!(!resp.content.is_empty());
    }

    #[tokio::test]
    async fn test_real_api_non_streaming_tool_call() {
        let api_key = match get_openrouter_api_key() {
            Some(key) if !key.is_empty() => key,
            _ => {
                eprintln!("OPENROUTER_API_KEY not found in ~/.bashrc, skipping");
                return;
            }
        };
        let mut client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key,
            model: "meta-llama/llama-3.1-8b-instruct".to_string(),
            temperature: 0.0,
            max_tokens: 20,
            stream_options: None,
        });
        let tools = vec![Tool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        }];
        let resp = create_message(
            &mut client,
            &[Message {
                role: MessageRole::User,
                content: "Read file test.txt".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &tools,
        )
        .await;
        assert_eq!(resp.error.code, 0);
        assert!(resp.tool_calls.len() > 0);
        assert_eq!(resp.tool_calls[0].function_name, "read_file");
    }

    #[tokio::test]
    async fn test_real_api_streaming_text() {
        let api_key = match get_openrouter_api_key() {
            Some(key) if !key.is_empty() => key,
            _ => {
                eprintln!("OPENROUTER_API_KEY not found in ~/.bashrc, skipping");
                return;
            }
        };
        let mut client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key,
            model: "meta-llama/llama-3.1-8b-instruct".to_string(),
            temperature: 0.0,
            max_tokens: 20,
            stream_options: None,
        });
        let mut received_text = String::new();
        let mut received_usage = false;
        let mut received_done = false;
        let resp = create_message_stream(
            &mut client,
            &[Message {
                role: MessageRole::User,
                content: "Say hello in one word".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &[],
            |chunk| {
                match chunk {
                    ApiStreamChunk::Text(t) => received_text.push_str(&t),
                    ApiStreamChunk::Usage { .. } => received_usage = true,
                    ApiStreamChunk::Done => received_done = true,
                    _ => {}
                }
                true
            },
        )
        .await;
        assert_eq!(resp.error.code, 0);
        assert!(!received_text.is_empty());
        assert!(received_done);
    }

    #[tokio::test]
    async fn test_real_api_streaming_tool_call() {
        let api_key = match get_openrouter_api_key() {
            Some(key) if !key.is_empty() => key,
            _ => {
                eprintln!("OPENROUTER_API_KEY not found in ~/.bashrc, skipping");
                return;
            }
        };
        let mut client = new_client(ApiClientConfig {
            base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key,
            model: "meta-llama/llama-3.1-8b-instruct".to_string(),
            temperature: 0.0,
            max_tokens: 20,
            stream_options: None,
        });
        let tools = vec![Tool {
            name: "read_file".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }),
        }];
        let mut received_done = false;
        let resp = create_message_stream(
            &mut client,
            &[Message {
                role: MessageRole::User,
                content: "Read file test.txt".to_string(),
                tool_calls: Vec::new(),
                tool_call_id: String::new(),
                name: String::new(),
            }],
            &tools,
            |chunk| {
                if let ApiStreamChunk::Done = chunk {
                    received_done = true;
                }
                true
            },
        )
        .await;
        assert_eq!(resp.error.code, 0);
        assert!(resp.tool_calls.len() > 0);
        assert_eq!(resp.tool_calls[0].function_name, "read_file");
        assert!(received_done);
    }

    fn get_openrouter_api_key() -> Option<String> {
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            let trimmed = key.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        let home = std::env::var("HOME").ok()?;
        let bashrc = std::fs::read_to_string(format!("{}/.bashrc", home)).ok()?;
        for line in bashrc.lines() {
            if let Some(rest) = line.strip_prefix("export OPENROUTER_API_KEY=") {
                let trimmed = rest.trim().trim_matches('\'').trim_matches('"').to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
        None
    }
}