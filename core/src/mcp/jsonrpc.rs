use serde_json::{Value, json};

pub fn build_request(method: &str, params: Option<&Value>, id: i64) -> String {
    let mut obj = json!({
        "jsonrpc": "2.0",
        "method": method,
        "id": id
    });
    if let Some(params) = params
        && !params.is_null()
    {
        obj["params"] = params.clone();
    }
    obj.to_string()
}

pub fn build_notification(method: &str, params: Option<&Value>) -> String {
    let mut obj = json!({
        "jsonrpc": "2.0",
        "method": method,
    });
    if let Some(params) = params
        && !params.is_null()
    {
        obj["params"] = params.clone();
    }
    obj.to_string()
}

pub fn parse_response(json_str: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(json_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_build_request_with_params() {
        let params = json!({"key": "value"});
        let result = build_request("test.method", Some(&params), 1);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "test.method");
        assert_eq!(v["id"], 1);
        assert_eq!(v["params"]["key"], "value");
    }

    #[test]
    fn test_build_request_null_params_omitted() {
        let result = build_request("test.method", None, 1);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "test.method");
        assert_eq!(v["id"], 1);
        assert!(!v.as_object().unwrap().contains_key("params"));
    }

    #[test]
    fn test_build_request_null_value_params_omitted() {
        let result = build_request("test.method", Some(&Value::Null), 1);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(!v.as_object().unwrap().contains_key("params"));
    }

    #[test]
    fn test_build_request_empty_object_retains_params() {
        let params = json!({});
        let result = build_request("test.method", Some(&params), 1);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(v["params"].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_build_request_empty_array_retains_params() {
        let params = json!([]);
        let result = build_request("test.method", Some(&params), 1);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(v["params"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_build_request_special_chars_in_method() {
        let result = build_request("mcp.tools/call", None, 42);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["method"], "mcp.tools/call");
        assert_eq!(v["id"], 42);
    }

    #[test]
    fn test_build_notification_with_params() {
        let params = json!({"key": "value"});
        let result = build_notification("test.notify", Some(&params));
        let v: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "test.notify");
        assert_eq!(v["params"]["key"], "value");
        assert!(!v.as_object().unwrap().contains_key("id"));
    }

    #[test]
    fn test_build_notification_null_params_omitted() {
        let result = build_notification("test.notify", None);
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(!v.as_object().unwrap().contains_key("params"));
        assert!(!v.as_object().unwrap().contains_key("id"));
    }

    #[test]
    fn test_build_notification_null_value_params_omitted() {
        let result = build_notification("test.notify", Some(&Value::Null));
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(!v.as_object().unwrap().contains_key("params"));
    }

    #[test]
    fn test_build_notification_empty_object_retains_params() {
        let params = json!({});
        let result = build_notification("test.notify", Some(&params));
        let v: Value = serde_json::from_str(&result).unwrap();
        assert!(v["params"].as_object().unwrap().is_empty());
    }

    #[test]
    fn test_parse_response_valid_result() {
        let json_str = r#"{"jsonrpc":"2.0","result":{"key":"value"},"id":1}"#;
        let v = parse_response(json_str).unwrap();
        assert_eq!(v["result"]["key"], "value");
        assert_eq!(v["id"], 1);
    }

    #[test]
    fn test_parse_response_valid_error() {
        let json_str =
            r#"{"jsonrpc":"2.0","error":{"code":-32601,"message":"Method not found"},"id":1}"#;
        let v = parse_response(json_str).unwrap();
        assert_eq!(v["error"]["code"], -32601);
    }

    #[test]
    fn test_parse_response_empty_object() {
        let v = parse_response("{}").unwrap();
        assert!(v.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_parse_response_empty_array() {
        let v = parse_response("[]").unwrap();
        assert!(v.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_parse_response_empty_string_fails() {
        let result = parse_response("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_response_invalid_json_fails() {
        let result = parse_response("not json");
        assert!(result.is_err());
    }
}
