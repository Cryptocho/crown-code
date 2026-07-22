use std::collections::HashMap;
use std::time::Duration;

pub use crate::mcp::sse::SseEvent;
use crate::mcp::sse::SseParser;

pub const DEFAULT_HTTP_TIMEOUT_MS: u64 = 30_000;
pub const MAX_RESPONSE_SIZE: usize = 10 * 1024 * 1024;
pub const SSE_READ_TIMEOUT_MS: u64 = 120_000;

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status_code: i32,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub error: String,
    pub events: Vec<SseEvent>,
}

pub struct HttpTransport {
    pub base_url: String,
    pub bearer_token: String,
    client: reqwest::Client,
    pub connected: bool,
    pub last_error: String,
}

impl HttpTransport {
    pub fn new(url: &str, bearer_token: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(SSE_READ_TIMEOUT_MS / 1000))
            .build()
            .expect("failed to build reqwest client");

        HttpTransport {
            base_url: url.to_string(),
            bearer_token: bearer_token.to_string(),
            client,
            connected: true,
            last_error: String::new(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn close(&mut self) {
        self.connected = false;
    }

    pub async fn post_json(&mut self, json_body: &str) -> HttpResponse {
        let response = match self
            .client
            .post(&self.base_url)
            .bearer_auth(&self.bearer_token)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .body(json_body.to_string())
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.last_error = format!("request failed: {}", e);
                return HttpResponse {
                    status_code: 0,
                    headers: HashMap::new(),
                    body: String::new(),
                    error: self.last_error.clone(),
                    events: Vec::new(),
                };
            }
        };

        let status_code = response.status().as_u16() as i32;
        let headers: HashMap<String, String> = response
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_lowercase(),
                    v.to_str().unwrap_or("").to_string(),
                )
            })
            .collect();

        let content_type = headers.get("content-type").cloned().unwrap_or_default();

        if content_type.starts_with("text/event-stream") {
            let bytes = match response.bytes().await {
                Ok(b) => b,
                Err(e) => {
                    return HttpResponse {
                        status_code,
                        headers,
                        body: String::new(),
                        error: format!("read error: {}", e),
                        events: Vec::new(),
                    };
                }
            };
            let chunk = String::from_utf8_lossy(&bytes);
            let mut parser = SseParser::new();
            let events = parser.feed(&chunk);
            let flush_events = parser.flush();
            let mut all_events = events;
            all_events.extend(flush_events);
            return HttpResponse {
                status_code,
                headers,
                body: String::new(),
                error: String::new(),
                events: all_events,
            };
        }

        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                return HttpResponse {
                    status_code,
                    headers,
                    body: String::new(),
                    error: format!("read error: {}", e),
                    events: Vec::new(),
                };
            }
        };

        HttpResponse {
            status_code,
            headers,
            body,
            error: String::new(),
            events: Vec::new(),
        }
    }

    pub async fn post_json_stream(
        &mut self,
        json_body: &str,
        mut on_event: impl FnMut(SseEvent) -> bool,
    ) -> (i32, String) {
        let mut response = match self
            .client
            .post(&self.base_url)
            .bearer_auth(&self.bearer_token)
            .header("Accept", "text/event-stream")
            .header("Content-Type", "application/json")
            .body(json_body.to_string())
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                self.last_error = format!("request failed: {}", e);
                return (0, self.last_error.clone());
            }
        };

        let status_code = response.status().as_u16() as i32;
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        if !content_type.starts_with("text/event-stream") {
            // Read the body to extract the actual error message from the API.
            // Most OpenAI-compatible APIs return errors as application/json
            // even when streaming was requested.
            let body = match response.text().await {
                Ok(b) => b,
                Err(e) => {
                    return (
                        status_code,
                        format!(
                            "expected text/event-stream but got: {} (body read error: {})",
                            content_type, e
                        ),
                    );
                }
            };
            // Try to extract a meaningful error message from the JSON body
            let detail = if let Ok(root) = serde_json::from_str::<serde_json::Value>(&body) {
                if let Some(err) = root.get("error") {
                    if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                        msg.to_string()
                    } else {
                        err.to_string()
                    }
                } else {
                    // Non-error JSON (e.g. full non-streaming response) — truncate for readability
                    if body.len() > 500 {
                        let truncated: String = body.chars().take(500).collect();
                        format!("{}...(truncated, {} bytes total)", truncated, body.len())
                    } else {
                        body.clone()
                    }
                }
            } else {
                if body.len() > 500 {
                    let truncated: String = body.chars().take(500).collect();
                    format!("{}...(truncated, {} bytes total)", truncated, body.len())
                } else {
                    body.clone()
                }
            };
            return (
                status_code,
                format!(
                    "expected text/event-stream but got: {} — {}",
                    content_type, detail
                ),
            );
        }

        let mut parser = SseParser::new();
        loop {
            match response.chunk().await {
                Ok(Some(bytes)) => {
                    let chunk = String::from_utf8_lossy(&bytes);
                    for evt in parser.feed(&chunk) {
                        if !on_event(evt) {
                            return (status_code, String::new());
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    return (status_code, format!("read error: {}", e));
                }
            }
        }
        for evt in parser.flush() {
            if !on_event(evt) {
                break;
            }
        }

        (status_code, String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_transport() {
        let t = HttpTransport::new("http://localhost:8080", "test-token");
        assert!(t.is_connected());
        assert_eq!(t.base_url, "http://localhost:8080");
        assert_eq!(t.bearer_token, "test-token");
    }

    #[test]
    fn test_close_transport() {
        let mut t = HttpTransport::new("http://localhost:8080", "");
        assert!(t.is_connected());
        t.close();
        assert!(!t.is_connected());
    }

    #[tokio::test]
    async fn test_post_json_connection_refused() {
        let mut t = HttpTransport::new("http://127.0.0.1:1", "token");
        let result = t.post_json(r#"{"jsonrpc":"2.0","method":"ping"}"#).await;
        assert_eq!(result.status_code, 0);
        assert!(!result.error.is_empty());
    }

    #[tokio::test]
    async fn test_post_json_stream_connection_refused() {
        let mut t = HttpTransport::new("http://127.0.0.1:1", "token");
        let (status, error) = t
            .post_json_stream(r#"{"jsonrpc":"2.0","method":"ping"}"#, |_| true)
            .await;
        assert_eq!(status, 0);
        assert!(!error.is_empty());
    }

    #[test]
    fn test_close_idempotent() {
        let mut t = HttpTransport::new("http://localhost:8080", "");
        t.close();
        assert!(!t.is_connected());
        t.close();
        assert!(!t.is_connected());
    }

    #[test]
    fn test_bearer_token_stored() {
        let t = HttpTransport::new("http://localhost:8080", "my-secret-token");
        assert_eq!(t.bearer_token, "my-secret-token");
    }

    #[test]
    fn test_post_json_sse_response_detection() {
        let mut parser = SseParser::new();
        let events = parser.feed("data: hello\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }

    #[test]
    fn test_http_response_struct_fields() {
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());
        let resp = HttpResponse {
            status_code: 200,
            headers: headers.clone(),
            body: r#"{"result":"ok"}"#.to_string(),
            error: String::new(),
            events: Vec::new(),
        };
        assert_eq!(resp.status_code, 200);
        assert_eq!(resp.headers["content-type"], "application/json");
        assert_eq!(resp.body, r#"{"result":"ok"}"#);
        assert!(resp.error.is_empty());
        assert!(resp.events.is_empty());
    }

    #[test]
    fn test_http_transport_url_with_path() {
        let t = HttpTransport::new("http://host:8080/mcp/v1", "");
        assert_eq!(t.base_url, "http://host:8080/mcp/v1");
    }

    // ── wiremock-based integration tests ──────────────────────────────
    mod mock_tests {
        use super::*;
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        // -- Content-Type header tests ------------------------------------

        #[tokio::test]
        async fn test_post_json_sends_content_type_json() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/test"))
                .and(header("Content-Type", "application/json"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "result": "ok"
                })))
                .expect(1)
                .mount(&server)
                .await;

            let url = format!("{}/test", server.uri());
            let mut t = HttpTransport::new(&url, "token");
            let resp = t.post_json(r#"{"key":"value"}"#).await;
            assert_eq!(resp.status_code, 200);
            assert!(resp.error.is_empty());
        }

        #[tokio::test]
        async fn test_post_json_stream_sends_content_type_json() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/stream"))
                .and(header("Content-Type", "application/json"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw("data: [DONE]\n\n", "text/event-stream"),
                )
                .expect(1)
                .mount(&server)
                .await;

            let url = format!("{}/stream", server.uri());
            let mut t = HttpTransport::new(&url, "token");
            let mut events = Vec::new();
            let (status, err) = t
                .post_json_stream(r#"{"stream":true}"#, |evt| {
                    events.push(evt);
                    true
                })
                .await;
            assert_eq!(status, 200);
            assert!(err.is_empty(), "should succeed, got: {}", err);
        }

        #[tokio::test]
        async fn test_post_json_sends_bearer_token() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(header("Authorization", "Bearer my-secret"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "ok": true
                })))
                .expect(1)
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "my-secret");
            let resp = t.post_json("{}").await;
            assert_eq!(resp.status_code, 200);
        }

        // -- post_json_stream JSON error body tests -----------------------

        #[tokio::test]
        async fn test_post_json_stream_json_error_extracts_error_message() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(401)
                        .insert_header("content-type", "application/json")
                        .set_body_json(serde_json::json!({
                            "error": {
                                "message": "Invalid API key provided: sk-***",
                                "type": "invalid_request_error",
                                "code": "invalid_api_key"
                            }
                        })),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "bad-key");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 401);
            assert!(
                err.contains("Invalid API key provided"),
                "should extract error.message, got: {}",
                err
            );
            assert!(
                err.contains("application/json"),
                "should mention content-type, got: {}",
                err
            );
        }

        #[tokio::test]
        async fn test_post_json_stream_json_error_returns_real_status_code() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(429)
                        .insert_header("content-type", "application/json")
                        .set_body_json(serde_json::json!({
                            "error": {
                                "message": "Rate limit exceeded",
                                "type": "rate_limit_error"
                            }
                        })),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 429, "should return real HTTP status, not 0");
            assert!(err.contains("Rate limit exceeded"));
        }

        #[tokio::test]
        async fn test_post_json_stream_json_error_without_message_field() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(400)
                        .insert_header("content-type", "application/json")
                        .set_body_json(serde_json::json!({
                            "error": "bad request"
                        })),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 400);
            // error is not a message string, so it falls back to err.to_string()
            assert!(
                err.contains("bad request"),
                "should include the error value, got: {}",
                err
            );
        }

        #[tokio::test]
        async fn test_post_json_stream_json_body_without_error_field() {
            let server = MockServer::start().await;
            let body = serde_json::json!({
                "id": "chatcmpl-123",
                "choices": [{"message": {"content": "hello"}}]
            });
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .insert_header("content-type", "application/json")
                        .set_body_json(&body),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 200);
            // Non-error JSON but not SSE — should include the body as detail
            assert!(
                err.contains("chatcmpl-123"),
                "should include raw body, got: {}",
                err
            );
        }

        #[tokio::test]
        async fn test_post_json_stream_large_json_body_truncated() {
            let server = MockServer::start().await;
            // Build a body > 500 chars with no "error" field
            let large_value = "x".repeat(1000);
            let body = serde_json::json!({
                "data": large_value
            });
            let body_str = body.to_string();
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string(&body_str)
                        .insert_header("content-type", "application/json"),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 200);
            assert!(
                err.contains("truncated"),
                "should indicate truncation, got: {}",
                err
            );
            let expected_size = body_str.len();
            assert!(
                err.contains(&format!("{} bytes total", expected_size)),
                "should show actual body size ({}), got: {}",
                expected_size,
                err
            );
        }

        #[tokio::test]
        async fn test_post_json_stream_non_json_error_body() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(502)
                        .insert_header("content-type", "text/plain")
                        .set_body_string("Bad Gateway"),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let (status, err) = t.post_json_stream("{}", |_| true).await;
            assert_eq!(status, 502);
            assert!(
                err.contains("Bad Gateway"),
                "should include raw body for non-JSON, got: {}",
                err
            );
        }

        #[tokio::test]
        async fn test_post_json_stream_sse_success() {
            let server = MockServer::start().await;
            let sse_body =
                "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: [DONE]\n\n";
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_raw(sse_body, "text/event-stream"),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let mut event_count = 0;
            let (status, err) = t
                .post_json_stream("{}", |_evt| {
                    event_count += 1;
                    true
                })
                .await;
            assert_eq!(status, 200);
            assert!(err.is_empty());
            assert!(
                event_count >= 2,
                "should get at least 2 events (data + DONE), got: {}",
                event_count
            );
        }

        // -- post_json JSON error body tests -----------------------------

        #[tokio::test]
        async fn test_post_json_returns_body_on_non_200() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(400)
                        .insert_header("content-type", "application/json")
                        .set_body_json(serde_json::json!({
                            "error": {
                                "message": "Invalid model: gpt-99",
                                "type": "invalid_request_error"
                            }
                        })),
                )
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let resp = t.post_json("{}").await;
            assert_eq!(resp.status_code, 400);
            assert!(
                resp.body.contains("Invalid model"),
                "post_json should include error body, got: {}",
                resp.body
            );
        }

        #[tokio::test]
        async fn test_post_json_sse_response_parsed() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\ndata: [DONE]\n\n",
                    "text/event-stream",
                ))
                .mount(&server)
                .await;

            let mut t = HttpTransport::new(&server.uri(), "token");
            let resp = t.post_json("{}").await;
            assert_eq!(resp.status_code, 200);
            assert!(resp.error.is_empty());
            assert!(
                resp.events.len() >= 2,
                "should parse SSE events, got: {}",
                resp.events.len()
            );
        }
    }
}
