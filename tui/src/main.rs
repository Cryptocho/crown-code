mod app_event;
mod event;
mod history_cell;
mod renderable;
mod tui;

use crate::tui::Tui;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut tui = Tui::init()?;
    tui.draw(|f| {
        let area = f.area();
        let block = ratatui::widgets::Block::default()
            .title("crown-code")
            .borders(ratatui::widgets::Borders::ALL);
        f.render_widget(block, area);
    })?;

    loop {
        if let Some(event::TuiEvent::Key(k)) = tui.event_receiver().recv().await
            && (k.code == crossterm::event::KeyCode::Char('q')
                || k.code == crossterm::event::KeyCode::Esc)
        {
            break;
        }
    }

    Ok(())
}
