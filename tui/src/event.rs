use crossterm::event::{KeyEvent, MouseEvent};

#[derive(Debug)]
pub enum TuiEvent {
    Key(KeyEvent),
    Paste(String),
    Mouse(MouseEvent),
    Resize,
    Draw,
}
