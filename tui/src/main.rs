mod app;
mod app_event;
mod chatwidget;
mod event;
mod history_cell;
mod ipc;
mod keymap;
mod renderable;
mod tui;
mod ui;

use std::time::Duration;

use crate::app::App;
use crate::app_event::{AppEvent, AppEventSender};
use crate::event::TuiEvent;
use crate::ipc::{IpcClient, IpcEventReader};
use crate::tui::Tui;
use crown_core::ipc::message::JsonRpcMessage;
use crown_core::ipc::transport::resolve_socket_path;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let socket_path = resolve_socket_path(
        args.iter()
            .position(|a| a == "--socket-path")
            .and_then(|i| args.get(i + 1))
            .map(|s| s.as_str()),
    );

    let (ipc, ipc_reader) = match IpcClient::connect(&socket_path).await {
        Ok(pair) => pair,
        Err(e) => {
            eprintln!("Failed to connect to crown-core daemon at {socket_path}: {e}");
            eprintln!("Make sure crown-core is running first.");
            std::process::exit(1);
        }
    };

    let session_result = ipc
        .send_request(
            "create_session",
            serde_json::json!({
                "cwd": std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string()),
            }),
        )
        .await;
    let session_id = match session_result {
        Ok(val) => val["session_id"]
            .as_str()
            .unwrap_or("sess_unknown")
            .to_string(),
        Err(e) => {
            eprintln!("Failed to create session: {e}");
            std::process::exit(1);
        }
    };

    let mut tui = Tui::init()?;

    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = Tui::restore();
        original_hook(info);
    }));

    let (app_event_tx, mut app_event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut app = App::new(session_id.clone(), AppEventSender::new(app_event_tx));

    let mut draw_interval = tokio::time::interval(Duration::from_millis(50));

    let mut ipc = Some(ipc);
    let mut ipc_reader = Some(ipc_reader);

    loop {
        tokio::select! {
            biased;

            Some(tui_event) = tui.event_receiver().recv() => {
                match tui_event {
                    TuiEvent::Key(key) => {
                        app.handle_key(key);
                    }
                    TuiEvent::Paste(text) => {
                        app.handle_paste(&text);
                    }
                    TuiEvent::Resize => {
                        app.needs_redraw = true;
                    }
                    TuiEvent::Draw => {}
                }
            }

            msg = maybe_read_message(&mut ipc_reader) => {
                if let Some(m) = msg {
                    app.handle_ipc_message(m);
                } else {
                    ipc = None;
                    ipc_reader = None;
                    app.handle_app_event(AppEvent::IpcDisconnected);
                }
            }

            Some(event) = app_event_rx.recv() => {
                match &event {
                    AppEvent::UserMessageSent(text) => {
                        let text = text.clone();
                        app.handle_app_event(event);
                        if let Some(ref ipc) = ipc {
                            let start = std::time::Instant::now();
                            let _ = ipc.send_request(
                                "user_message",
                                serde_json::json!({
                                    "session_id": app.session_id.as_deref().unwrap_or(""),
                                    "content": text,
                                }),
                            ).await;
                            let elapsed = start.elapsed().as_millis() as u64;
                            app.api_latencies.push_back(elapsed);
                            if app.api_latencies.len() > 5 {
                                app.api_latencies.pop_front();
                            }
                        }
                    }
                    AppEvent::CancelRequested => {
                        app.handle_app_event(AppEvent::CancelRequested);
                        if let Some(ref ipc) = ipc {
                            let _ = ipc.send_request(
                                "cancel",
                                serde_json::json!({
                                    "session_id": app.session_id.as_deref().unwrap_or(""),
                                }),
                            ).await;
                        }
                    }
                    AppEvent::ReconnectRequested => {
                        match IpcClient::connect(&socket_path).await {
                            Ok((new_client, new_reader)) => {
                                match new_client.send_request("create_session", serde_json::json!({
                                    "cwd": std::env::current_dir()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|_| ".".to_string()),
                                })).await {
                                    Ok(val) => {
                                        let sid = val["session_id"]
                                            .as_str()
                                            .unwrap_or("sess_unknown")
                                            .to_string();
                                        ipc = Some(new_client);
                                        ipc_reader = Some(new_reader);
                                        app.handle_app_event(AppEvent::IpcReconnected {
                                            session_id: sid,
                                        });
                                    }
                                    Err(e) => {
                                        app.handle_app_event(AppEvent::ReconnectFailed {
                                            reason: format!("create_session failed: {e}"),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                app.handle_app_event(AppEvent::ReconnectFailed {
                                    reason: format!("{e}"),
                                });
                            }
                        }
                    }
                    _ => {
                        app.handle_app_event(event);
                    }
                }
            }

            _ = draw_interval.tick() => {
                if app.needs_redraw {
                    if let Err(e) = tui.draw(|f| ui::render(f, &app)) {
                        eprintln!("draw error: {e}");
                    }
                    app.needs_redraw = false;
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    if let Some(ref ipc) = ipc {
        let _ = ipc
            .send_notification(
                "destroy_session",
                serde_json::json!({"session_id": session_id}),
            )
            .await;
    }
    let _ = Tui::restore();
    Ok(())
}

async fn maybe_read_message(reader: &mut Option<IpcEventReader>) -> Option<JsonRpcMessage> {
    match reader {
        Some(r) => r.read_message().await,
        None => std::future::pending().await,
    }
}
