use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Clone, PartialEq)]
pub enum KeyAction {
    Quit,
    Cancel,
    CycleAgentMode,
    SubmitMessage,
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToBottom,
    ToggleToolExpand,
    FocusNext,
    None,
}

pub fn map_input_key(key: KeyEvent) -> KeyAction {
    if key.kind != KeyEventKind::Press {
        return KeyAction::None;
    }
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Quit,
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => KeyAction::Quit,
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => KeyAction::Cancel,
        (KeyModifiers::NONE, KeyCode::Esc) => KeyAction::Cancel,
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => KeyAction::CycleAgentMode,

        (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::SubmitMessage,
        (KeyModifiers::NONE, KeyCode::Tab) => KeyAction::FocusNext,

        _ => KeyAction::None,
    }
}

pub fn map_chat_key(key: KeyEvent) -> KeyAction {
    if key.kind != KeyEventKind::Press {
        return KeyAction::None;
    }
    match (key.modifiers, key.code) {
        (KeyModifiers::CONTROL, KeyCode::Char('c')) => KeyAction::Quit,
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => KeyAction::Quit,
        (KeyModifiers::CONTROL, KeyCode::Char('x')) => KeyAction::Cancel,
        (KeyModifiers::NONE, KeyCode::Esc) => KeyAction::Cancel,
        (KeyModifiers::CONTROL, KeyCode::Char('p')) => KeyAction::CycleAgentMode,

        (KeyModifiers::NONE, KeyCode::PageUp) => KeyAction::ScrollUp(10),
        (KeyModifiers::NONE, KeyCode::PageDown) => KeyAction::ScrollDown(10),
        (KeyModifiers::CONTROL, KeyCode::End) => KeyAction::ScrollToBottom,
        (KeyModifiers::SHIFT, KeyCode::Up) => KeyAction::ScrollUp(1),
        (KeyModifiers::SHIFT, KeyCode::Down) => KeyAction::ScrollDown(1),

        (KeyModifiers::NONE, KeyCode::Enter) => KeyAction::ToggleToolExpand,
        (KeyModifiers::NONE, KeyCode::Char(' ')) => KeyAction::ToggleToolExpand,

        (KeyModifiers::NONE, KeyCode::Tab) => KeyAction::FocusNext,

        _ => KeyAction::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(modifiers: KeyModifiers, code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn test_release_event_returns_none() {
        let mut k = key(KeyModifiers::CONTROL, KeyCode::Char('c'));
        k.kind = KeyEventKind::Release;
        assert_eq!(map_input_key(k), KeyAction::None);
        assert_eq!(map_chat_key(k), KeyAction::None);
    }

    #[test]
    fn test_input_ctrl_c_quit() {
        assert_eq!(
            map_input_key(key(KeyModifiers::CONTROL, KeyCode::Char('c'))),
            KeyAction::Quit
        );
    }

    #[test]
    fn test_input_ctrl_d_quit() {
        assert_eq!(
            map_input_key(key(KeyModifiers::CONTROL, KeyCode::Char('d'))),
            KeyAction::Quit
        );
    }

    #[test]
    fn test_input_ctrl_x_cancel() {
        assert_eq!(
            map_input_key(key(KeyModifiers::CONTROL, KeyCode::Char('x'))),
            KeyAction::Cancel
        );
    }

    #[test]
    fn test_input_esc_cancel() {
        assert_eq!(
            map_input_key(key(KeyModifiers::NONE, KeyCode::Esc)),
            KeyAction::Cancel
        );
    }

    #[test]
    fn test_input_ctrl_p_cycle_mode() {
        assert_eq!(
            map_input_key(key(KeyModifiers::CONTROL, KeyCode::Char('p'))),
            KeyAction::CycleAgentMode
        );
    }

    #[test]
    fn test_input_enter_submit() {
        assert_eq!(
            map_input_key(key(KeyModifiers::NONE, KeyCode::Enter)),
            KeyAction::SubmitMessage
        );
    }

    #[test]
    fn test_input_tab_focus() {
        assert_eq!(
            map_input_key(key(KeyModifiers::NONE, KeyCode::Tab)),
            KeyAction::FocusNext
        );
    }

    #[test]
    fn test_input_char_none() {
        assert_eq!(
            map_input_key(key(KeyModifiers::NONE, KeyCode::Char('a'))),
            KeyAction::None
        );
    }

    #[test]
    fn test_input_backspace_none() {
        assert_eq!(
            map_input_key(key(KeyModifiers::NONE, KeyCode::Backspace)),
            KeyAction::None
        );
    }

    #[test]
    fn test_chat_page_up() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::PageUp)),
            KeyAction::ScrollUp(10)
        );
    }

    #[test]
    fn test_chat_page_down() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::PageDown)),
            KeyAction::ScrollDown(10)
        );
    }

    #[test]
    fn test_chat_ctrl_end() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::CONTROL, KeyCode::End)),
            KeyAction::ScrollToBottom
        );
    }

    #[test]
    fn test_chat_shift_up() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::SHIFT, KeyCode::Up)),
            KeyAction::ScrollUp(1)
        );
    }

    #[test]
    fn test_chat_enter_toggle() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::Enter)),
            KeyAction::ToggleToolExpand
        );
    }

    #[test]
    fn test_chat_space_toggle() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::Char(' '))),
            KeyAction::ToggleToolExpand
        );
    }

    #[test]
    fn test_chat_tab_focus() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::Tab)),
            KeyAction::FocusNext
        );
    }

    #[test]
    fn test_chat_char_none() {
        assert_eq!(
            map_chat_key(key(KeyModifiers::NONE, KeyCode::Char('a'))),
            KeyAction::None
        );
    }

    #[test]
    fn test_global_shortcuts_consistent() {
        let global_keys = vec![
            key(KeyModifiers::CONTROL, KeyCode::Char('c')),
            key(KeyModifiers::CONTROL, KeyCode::Char('d')),
            key(KeyModifiers::CONTROL, KeyCode::Char('x')),
            key(KeyModifiers::NONE, KeyCode::Esc),
            key(KeyModifiers::CONTROL, KeyCode::Char('p')),
        ];
        for k in global_keys {
            assert_eq!(map_input_key(k), map_chat_key(k), "global key mismatch: {k:?}");
        }
    }
}