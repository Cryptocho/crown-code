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

use crate::app::{App, FocusTarget};
use crate::app_event::AppEventSender;
use crate::event::TuiEvent;
use crate::history_cell::{AssistantMessageCell, UserMessageCell};
use crate::tui::Tui;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut tui = Tui::init()?;

    let (app_event_tx, _app_event_rx) = mpsc::unbounded_channel();
    let mut app = App::new("local".into(), AppEventSender::new(app_event_tx));
    app.model = "local".into();
    app.session_name = Some("我的测试项目".into());
    app.input_tokens = 1234;
    app.output_tokens = 567;
    app.cache_read_tokens = 890;
    app.api_latencies = std::collections::VecDeque::from([230, 180, 250]);
    app.needs_redraw = true;

    app.chat_widget.push_cell(Box::new(UserMessageCell {
        content: "读取 main.rs".into(),
    }));
    app.chat_widget.push_cell(Box::new(AssistantMessageCell {
        content: "好的，我来读取...\n这是第二行内容，用于测试多行显示效果。".into(),
        is_streaming: false,
    }));
    app.chat_widget
        .start_tool_call("c1", "read_file", "/tmp/test.rs");
    std::thread::sleep(std::time::Duration::from_millis(10));
    app.chat_widget
        .finish_tool_call("c1", "read_file", "content here", false);
    app.chat_widget.push_cell(Box::new(AssistantMessageCell {
        content: "文件已读取，内容如下。".into(),
        is_streaming: false,
    }));

    loop {
        if app.needs_redraw {
            tui.draw(|f| {
                ui::render(f, &app);
            })?;
            app.needs_redraw = false;
        }

        if let Some(event) = tui.event_receiver().recv().await {
            match event {
                TuiEvent::Key(k) => {
                    if app.focus == FocusTarget::Input {
                        let action = keymap::map_input_key(k);
                        match action {
                            keymap::KeyAction::Quit => break,
                            keymap::KeyAction::FocusNext => {
                                app.focus = FocusTarget::ChatPanel;
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::CycleAgentMode => {
                                app.agent_mode = app.agent_mode.cycle();
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::SubmitMessage => {
                                let msg = app.chat_widget.take_input();
                                if !msg.trim().is_empty() {
                                    app.chat_widget
                                        .push_cell(Box::new(UserMessageCell { content: msg }));
                                }
                                app.needs_redraw = true;
                            }
                            _ => {
                                app.chat_widget.input_key(k);
                                app.needs_redraw = true;
                            }
                        }
                    } else {
                        let action = keymap::map_chat_key(k);
                        match action {
                            keymap::KeyAction::Quit => break,
                            keymap::KeyAction::FocusNext => {
                                app.focus = FocusTarget::Input;
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::ScrollUp(n) => {
                                app.chat_widget.scroll_up(n);
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::ScrollDown(n) => {
                                app.chat_widget.scroll_down(n);
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::ScrollToBottom => {
                                app.chat_widget.scroll_to_bottom();
                                app.needs_redraw = true;
                            }
                            keymap::KeyAction::ToggleToolExpand => {
                                for (i, cell) in app.chat_widget.cells.iter().enumerate() {
                                    if cell.as_tool_call().is_some() {
                                        app.chat_widget.toggle_tool_expanded(i);
                                        break;
                                    }
                                }
                                app.needs_redraw = true;
                            }
                            _ => {
                                app.needs_redraw = true;
                            }
                        }
                    }
                }
                TuiEvent::Resize => {
                    app.needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    let _ = Tui::restore();
    Ok(())
}
