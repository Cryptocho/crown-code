use crown_core::api::types::ApiClientConfig;
use crown_core::ipc::server::IpcServer;
use crown_core::ipc::transport::resolve_socket_path;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    let socket_path = resolve_socket_path(
        args.iter()
            .position(|a| a == "--socket-path")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str()),
    );

    let config = ApiClientConfig {
        base_url: "http://localhost:11434/v1".to_string(),
        api_key: String::new(),
        model: "gemma4:e4b".to_string(),
        temperature: 0.0,
        max_tokens: 4096,
        stream_options: None,
    };

    let server = IpcServer::new(&socket_path, config).expect("failed to start IPC server");
    eprintln!("crown-core daemon started, listening on {socket_path}");

    tokio::select! {
        r = server.run() => { if let Err(e) = r { eprintln!("server error: {e}"); } }
        _ = tokio::signal::ctrl_c() => {
            eprintln!("shutting down...");
            let _ = server.shutdown().await;
        }
    }
}