use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcMessage {
    pub fn is_request(&self) -> bool {
        self.id.is_some() && self.method.is_some()
    }

    pub fn is_notification(&self) -> bool {
        self.id.is_none() && self.method.is_some()
    }

    pub fn is_response(&self) -> bool {
        self.id.is_some() && self.method.is_none()
    }

    pub fn to_line(&self) -> String {
        serde_json::to_string(self).expect("JsonRpcMessage serialization should not fail")
    }

    pub fn from_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }
}

pub fn make_request(id: u64, method: &str, params: Value) -> JsonRpcMessage {
    JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id: Some(id),
        method: Some(method.to_string()),
        params: Some(params),
        result: None,
        error: None,
    }
}

pub fn make_notification(method: &str, params: Value) -> JsonRpcMessage {
    JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id: None,
        method: Some(method.to_string()),
        params: Some(params),
        result: None,
        error: None,
    }
}

pub fn make_response(id: u64, result: Value) -> JsonRpcMessage {
    JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id: Some(id),
        method: None,
        params: None,
        result: Some(result),
        error: None,
    }
}

pub fn make_error_response(
    id: u64,
    code: i32,
    message: &str,
    data: Option<Value>,
) -> JsonRpcMessage {
    JsonRpcMessage {
        jsonrpc: "2.0".to_string(),
        id: Some(id),
        method: None,
        params: None,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
            data,
        }),
    }
}

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

pub const SESSION_NOT_FOUND: i32 = -31001;
pub const INVALID_SESSION_ID: i32 = -31002;

pub const METHOD_CREATE_SESSION: &str = "create_session";
pub const METHOD_USER_MESSAGE: &str = "user_message";
pub const METHOD_CANCEL: &str = "cancel";
pub const METHOD_DESTROY_SESSION: &str = "destroy_session";
pub const METHOD_LIST_SESSIONS: &str = "list_sessions";
pub const METHOD_SET_CONFIG: &str = "set_config";
pub const METHOD_SET_AGENT_MODE: &str = "set_agent_mode";

pub const METHOD_ASSISTANT_TEXT: &str = "assistant_text";
pub const METHOD_ASSISTANT_REASONING: &str = "assistant_reasoning";
pub const METHOD_TOOL_CALL_START: &str = "tool_call_start";
pub const METHOD_TOOL_RESULT: &str = "tool_result";
pub const METHOD_USAGE: &str = "usage";
pub const METHOD_TASK_DONE: &str = "task_done";
pub const METHOD_ERROR: &str = "error";
pub const METHOD_SESSION_CREATED: &str = "session_created";
pub const METHOD_SESSION_DESTROYED: &str = "session_destroyed";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_request_fields() {
        let params = serde_json::json!({"cwd": "/tmp"});
        let msg = make_request(1, "create_session", params);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "create_session");
        assert_eq!(json["params"]["cwd"], "/tmp");
        assert!(json.get("result").is_none());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_make_notification_omits_id() {
        let msg = make_notification("assistant_text", serde_json::json!({"delta": "hi"}));
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert!(json.get("id").is_none());
        assert_eq!(json["method"], "assistant_text");
    }

    #[test]
    fn test_make_response_fields() {
        let msg = make_response(5, serde_json::json!({"ok": true}));
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["id"], 5);
        assert_eq!(json["result"]["ok"], true);
        assert!(json.get("method").is_none());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn test_make_error_response_fields() {
        let data = serde_json::json!({"detail": "not found"});
        let msg = make_error_response(3, SESSION_NOT_FOUND, "session not found", Some(data));
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["id"], 3);
        assert_eq!(json["error"]["code"], SESSION_NOT_FOUND);
        assert_eq!(json["error"]["message"], "session not found");
        assert_eq!(json["error"]["data"]["detail"], "not found");
        assert!(json.get("result").is_none());
    }

    #[test]
    fn test_is_request_true_false() {
        let req = make_request(1, "ping", serde_json::json!({}));
        assert!(req.is_request());
        assert!(!req.is_notification());
        assert!(!req.is_response());

        let notif = make_notification("ping", serde_json::json!({}));
        assert!(!notif.is_request());
        assert!(notif.is_notification());
        assert!(!notif.is_response());

        let resp = make_response(1, serde_json::json!({}));
        assert!(!resp.is_request());
        assert!(!resp.is_notification());
        assert!(resp.is_response());
    }

    #[test]
    fn test_is_notification_true_false() {
        assert!(!make_request(1, "x", serde_json::json!({})).is_notification());
        assert!(make_notification("x", serde_json::json!({})).is_notification());
        assert!(!make_response(1, serde_json::json!({})).is_notification());
    }

    #[test]
    fn test_is_response_true_false() {
        assert!(!make_request(1, "x", serde_json::json!({})).is_response());
        assert!(!make_notification("x", serde_json::json!({})).is_response());
        assert!(make_response(1, serde_json::json!({})).is_response());
    }

    #[test]
    fn test_from_line_valid() {
        let line = r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
        let msg = JsonRpcMessage::from_line(line).unwrap();
        assert_eq!(msg.jsonrpc, "2.0");
        assert_eq!(msg.id, Some(1));
        assert_eq!(msg.method.as_deref(), Some("ping"));
    }

    #[test]
    fn test_from_line_invalid() {
        assert!(JsonRpcMessage::from_line("not json").is_err());
        assert!(JsonRpcMessage::from_line("").is_err());
    }

    #[test]
    fn test_to_line_from_line_roundtrip() {
        let original = make_request(42, "test_method", serde_json::json!({"key": "val"}));
        let line = original.to_line();
        let parsed = JsonRpcMessage::from_line(&line).unwrap();
        assert_eq!(parsed.jsonrpc, original.jsonrpc);
        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.method, original.method);
        assert_eq!(parsed.params, original.params);
    }

    #[test]
    fn test_deserialize_missing_optional_fields() {
        let line = r#"{"jsonrpc":"2.0"}"#;
        let msg = JsonRpcMessage::from_line(line).unwrap();
        assert_eq!(msg.jsonrpc, "2.0");
        assert!(msg.id.is_none());
        assert!(msg.method.is_none());
        assert!(msg.params.is_none());
        assert!(msg.result.is_none());
        assert!(msg.error.is_none());
    }

    #[test]
    fn test_error_data_present_and_absent() {
        let with_data = make_error_response(1, -1, "err", Some(serde_json::json!({"x": 1})));
        let json_with = serde_json::to_value(&with_data).unwrap();
        assert!(json_with["error"].get("data").is_some());

        let without_data = make_error_response(2, -1, "err", None);
        let json_without = serde_json::to_value(&without_data).unwrap();
        assert!(json_without["error"].get("data").is_none());
    }
}
