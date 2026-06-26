use bevy::{
    input::{keyboard::{Key, KeyboardInput}, ButtonState},
    prelude::*,
};
use bevy_ratatui::event::KeyEvent;
use crossterm::event::{
    KeyCode as CTCode, KeyEvent as CTKeyEvent, KeyEventKind, KeyEventState, KeyModifiers,
};

/// Converts Bevy keyboard events to crossterm KeyEvents and fires them as `KeyEvent`.
pub fn bevy_keyboard_to_key_event(
    mut keyboard: EventReader<KeyboardInput>,
    modifier_keys: Res<ButtonInput<KeyCode>>,
    mut key_events: EventWriter<KeyEvent>,
) {
    for input in keyboard.read() {
        if input.state != ButtonState::Pressed {
            continue;
        }

        let Some(code) = logical_key_to_crossterm(&input.logical_key, &modifier_keys) else {
            continue;
        };

        let mut modifiers = KeyModifiers::empty();
        if modifier_keys.pressed(KeyCode::ShiftLeft) || modifier_keys.pressed(KeyCode::ShiftRight) {
            modifiers |= KeyModifiers::SHIFT;
        }
        if modifier_keys.pressed(KeyCode::ControlLeft)
            || modifier_keys.pressed(KeyCode::ControlRight)
        {
            modifiers |= KeyModifiers::CONTROL;
        }
        if modifier_keys.pressed(KeyCode::AltLeft) || modifier_keys.pressed(KeyCode::AltRight) {
            modifiers |= KeyModifiers::ALT;
        }

        key_events.send(KeyEvent(CTKeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }));
    }
}

fn logical_key_to_crossterm(key: &Key, modifier_keys: &ButtonInput<KeyCode>) -> Option<CTCode> {
    let shift = modifier_keys.pressed(KeyCode::ShiftLeft)
        || modifier_keys.pressed(KeyCode::ShiftRight);

    match key {
        Key::Character(s) => {
            let ch = s.chars().next()?;
            Some(CTCode::Char(ch))
        }
        Key::Enter => Some(CTCode::Enter),
        Key::Escape => Some(CTCode::Esc),
        Key::Tab => {
            if shift {
                Some(CTCode::BackTab)
            } else {
                Some(CTCode::Tab)
            }
        }
        Key::Backspace => Some(CTCode::Backspace),
        Key::Delete => Some(CTCode::Delete),
        Key::ArrowUp => Some(CTCode::Up),
        Key::ArrowDown => Some(CTCode::Down),
        Key::ArrowLeft => Some(CTCode::Left),
        Key::ArrowRight => Some(CTCode::Right),
        Key::Home => Some(CTCode::Home),
        Key::End => Some(CTCode::End),
        Key::PageUp => Some(CTCode::PageUp),
        Key::PageDown => Some(CTCode::PageDown),
        Key::Insert => Some(CTCode::Insert),
        Key::Space => Some(CTCode::Char(' ')),
        Key::F1 => Some(CTCode::F(1)),
        Key::F2 => Some(CTCode::F(2)),
        Key::F3 => Some(CTCode::F(3)),
        Key::F4 => Some(CTCode::F(4)),
        Key::F5 => Some(CTCode::F(5)),
        Key::F6 => Some(CTCode::F(6)),
        Key::F7 => Some(CTCode::F(7)),
        Key::F8 => Some(CTCode::F(8)),
        Key::F9 => Some(CTCode::F(9)),
        Key::F10 => Some(CTCode::F(10)),
        Key::F11 => Some(CTCode::F(11)),
        Key::F12 => Some(CTCode::F(12)),
        _ => None,
    }
}
