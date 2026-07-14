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
        let response = match self
            .client
            .post(&self.base_url)
            .bearer_auth(&self.bearer_token)
            .header("Accept", "text/event-stream")
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
            return (
                0,
                format!("expected text/event-stream but got: {}", content_type),
            );
        }

        let mut parser = SseParser::new();
        match response.bytes().await {
            Ok(bytes) => {
                let chunk = String::from_utf8_lossy(&bytes);
                for evt in parser.feed(&chunk) {
                    if !on_event(evt) {
                        return (status_code, String::new());
                    }
                }
                for evt in parser.flush() {
                    if !on_event(evt) {
                        break;
                    }
                }
            }
            Err(e) => {
                return (status_code, format!("read error: {}", e));
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
        let (status, error) = t.post_json_stream(r#"{"jsonrpc":"2.0","method":"ping"}"#, |_| true).await;
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
}
