//! Shortcuts on keyboard layouts where `=` and `+` take Shift. DeckCraft names its shortcuts by
//! the US keys, as PowerPoint's lists do: Subscript is Cmd/Ctrl+=, Superscript Cmd/Ctrl+Shift+=
//! (the `+` of a US keyboard), Zoom In Cmd/Ctrl+=. The window system reports the character a key
//! makes, Shift included, so on a Japanese (JIS) keyboard, where `=` is Shift+- and `+` is
//! Shift+;, Cmd+Shift+- arrived as Shift+`=` and ran Superscript, and nothing reached Subscript
//! or Zoom In. On a US keyboard Cmd+Shift+= arrived as Shift+`+`, which matched nothing either.
//!
//! [`shortcut_key`] reads a key press the way the shortcut names mean it: a Shift that only made
//! the character (`=` typed as Shift+- on JIS) is not a modifier, and `+` typed with Shift on any
//! layout is Shift+= (US). Only the `=` and `+` characters are touched, and only when the key
//! that made them is known and is another key (the numeric keypad's `+` stays Zoom In).

use egui::Key;

/// The key and Shift state a key press is matched against shortcuts with. `key` is the
/// character's key, `physical` the key that was pressed (`None` when unknown: synthetic events,
/// the control channel), `shift` whether Shift was held.
pub fn shortcut_key(key: Key, physical: Option<Key>, shift: bool) -> (Key, bool) {
    match (key, physical) {
        // `=` that needed Shift on another key (JIS Shift+-, German Shift+0): Cmd+= as named.
        (Key::Equals, Some(p)) if shift && p != Key::Equals => (Key::Equals, false),
        // `+` that needed Shift (US Shift+=, JIS Shift+;): the shortcut tables write it Shift+=.
        (Key::Plus, Some(p)) if shift && p != Key::Plus => (Key::Equals, true),
        _ => (key, shift),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_that_only_made_the_character_is_dropped() {
        // JIS: Cmd+Shift+- makes `=` → Cmd+= (Subscript / Zoom In).
        assert_eq!(shortcut_key(Key::Equals, Some(Key::Minus), true), (Key::Equals, false));
        // JIS: Cmd+Shift+; makes `+` → Cmd+Shift+= (Superscript), as on a US keyboard.
        assert_eq!(shortcut_key(Key::Plus, Some(Key::Semicolon), true), (Key::Equals, true));
        // US: Cmd+Shift+= makes `+` → Cmd+Shift+= (Superscript).
        assert_eq!(shortcut_key(Key::Plus, Some(Key::Equals), true), (Key::Equals, true));
    }

    #[test]
    fn other_keys_are_left_alone() {
        // US Cmd+= and the keypad's + are what they say.
        assert_eq!(shortcut_key(Key::Equals, Some(Key::Equals), false), (Key::Equals, false));
        assert_eq!(shortcut_key(Key::Equals, Some(Key::Equals), true), (Key::Equals, true));
        assert_eq!(shortcut_key(Key::Plus, Some(Key::Plus), false), (Key::Plus, false));
        assert_eq!(shortcut_key(Key::Plus, Some(Key::Plus), true), (Key::Plus, true));
        // Unknown physical key (synthetic events, the control channel): unchanged.
        assert_eq!(shortcut_key(Key::Equals, None, true), (Key::Equals, true));
        assert_eq!(shortcut_key(Key::Plus, None, true), (Key::Plus, true));
        // Every other key, Shift or not.
        assert_eq!(shortcut_key(Key::Minus, Some(Key::Minus), true), (Key::Minus, true));
        assert_eq!(shortcut_key(Key::B, Some(Key::B), true), (Key::B, true));
        assert_eq!(shortcut_key(Key::CloseBracket, Some(Key::Backslash), true), (Key::CloseBracket, true));
    }
}
