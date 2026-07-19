pub mod chat;
pub mod input;
pub mod status;
pub mod streaming;
pub mod tools;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let buf = frame.buffer_mut();

    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

    let status_data = status::status_bar_data_from_app(app);
    status::render_status_bar(chunks[0], buf, &status_data);

    chat::render_chat_panel(chunks[1], buf, &app.chat_widget);

    let input_data = input::InputBarData {
        model: &app.model,
        agent_mode: &app.agent_mode,
        textarea: &app.chat_widget.textarea,
        focus: app.focus == crate::app::FocusTarget::Input,
        is_disconnected: app.is_disconnected(),
    };
    input::render_input_bar(chunks[2], buf, &input_data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::app_event::AppEventSender;
    use tokio::sync::mpsc;

    fn make_test_app() -> App {
        let (tx, _rx) = mpsc::unbounded_channel();
        App::new("test_session".into(), AppEventSender::new(tx))
    }

    #[test]
    fn test_full_render_no_panic() {
        let app = make_test_app();
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        let status_data = status::status_bar_data_from_app(&app);
        status::render_status_bar(chunks[0], &mut buf, &status_data);
        chat::render_chat_panel(chunks[1], &mut buf, &app.chat_widget);

        let input_data = input::InputBarData {
            model: &app.model,
            agent_mode: &app.agent_mode,
            textarea: &app.chat_widget.textarea,
            focus: true,
            is_disconnected: false,
        };
        input::render_input_bar(chunks[2], &mut buf, &input_data);
    }

    #[test]
    fn test_render_minimum_terminal_size() {
        let app = make_test_app();
        let area = ratatui::layout::Rect::new(0, 0, 20, 5);
        let mut buf = ratatui::buffer::Buffer::empty(area);

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

        let status_data = status::status_bar_data_from_app(&app);
        status::render_status_bar(chunks[0], &mut buf, &status_data);
        chat::render_chat_panel(chunks[1], &mut buf, &app.chat_widget);

        let input_data = input::InputBarData {
            model: &app.model,
            agent_mode: &app.agent_mode,
            textarea: &app.chat_widget.textarea,
            focus: false,
            is_disconnected: false,
        };
        input::render_input_bar(chunks[2], &mut buf, &input_data);
    }
}
