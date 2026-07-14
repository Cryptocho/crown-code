use crossterm::event::KeyEvent;

#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Paste(String),
    Resize,
}
