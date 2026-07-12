use crown_core::agent::r#loop::run_agent_loop;
use crown_core::api::types::ApiClientConfig;

fn main() {
    let config = ApiClientConfig {
        base_url: "http://localhost:11434/v1".to_string(),
        api_key: String::new(),
        model: "gemma4:e4b".to_string(),
        temperature: 0.0,
        max_tokens: 4096,
        stream_options: None,
    };
    run_agent_loop(config);
}