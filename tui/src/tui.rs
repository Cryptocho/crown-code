use std::io::{self, Stdout};

use crossterm::{
    event::{DisableBracketedPaste, EnableBracketedPaste, Event, EventStream},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use tokio::sync::mpsc;

use crate::event::TuiEvent;

pub type TuiBackend = CrosstermBackend<Stdout>;

pub struct Tui {
    terminal: Terminal<TuiBackend>,
    #[allow(dead_code)]
    event_tx: mpsc::UnboundedSender<TuiEvent>,
    event_rx: mpsc::UnboundedReceiver<TuiEvent>,
}

impl Tui {
    pub fn init() -> anyhow::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;

        let (event_tx, event_rx) = mpsc::unbounded_channel::<TuiEvent>();

        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut event_stream = EventStream::new();
            loop {
                match event_stream.next().await {
                    Some(Ok(event)) => {
                        let tui_event = match event {
                            Event::Key(key) => Some(TuiEvent::Key(key)),
                            Event::Paste(text) => Some(TuiEvent::Paste(text)),
                            Event::Resize(_, _) => Some(TuiEvent::Resize),
                            _ => None,
                        };
                        if let Some(e) = tui_event
                            && tx.send(e).is_err()
                        {
                            break;
                        }
                    }
                    Some(Err(_)) => continue,
                    None => break,
                }
            }
        });

        Ok(Self {
            terminal,
            event_tx,
            event_rx,
        })
    }

    pub fn restore() -> anyhow::Result<()> {
        crossterm::terminal::disable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, LeaveAlternateScreen, DisableBracketedPaste)?;
        Ok(())
    }

    pub fn enter_alt_screen(&mut self) -> anyhow::Result<()> {
        execute!(
            self.terminal.backend_mut(),
            EnterAlternateScreen,
            EnableBracketedPaste
        )?;
        Ok(())
    }

    pub fn leave_alt_screen(&mut self) -> anyhow::Result<()> {
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableBracketedPaste
        )?;
        Ok(())
    }

    pub fn draw<F>(&mut self, f: F) -> anyhow::Result<()>
    where
        F: FnOnce(&mut Frame),
    {
        self.terminal.draw(f)?;
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        let area = self.terminal.size().expect("failed to get terminal size");
        (area.width, area.height)
    }

    pub fn event_receiver(&mut self) -> &mut mpsc::UnboundedReceiver<TuiEvent> {
        &mut self.event_rx
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = Self::restore();
    }
}
