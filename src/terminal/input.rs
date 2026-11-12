use gpui_kit::Keystroke;

/// Encode non-text keys. Printable text is handled by the platform input/IME API.
pub(super) fn key_bytes(key: &Keystroke, application_cursor: bool) -> Option<Vec<u8>> {
    if key.modifiers.platform {
        return None;
    }
    let modifiers = 1
        + u8::from(key.modifiers.shift)
        + 2 * u8::from(key.modifiers.alt)
        + 4 * u8::from(key.modifiers.control);
    let sequence = match key.key.as_str() {
        "enter" => "\r".into(),
        "tab" if key.modifiers.shift => "\x1b[Z".into(),
        "tab" => "\t".into(),
        "escape" => "\x1b".into(),
        "backspace" if key.modifiers.alt => "\x1b\x7f".into(),
        "backspace" => "\x7f".into(),
        "up" | "down" | "right" | "left" | "home" | "end" => {
            let code = match key.key.as_str() {
                "up" => 'A',
                "down" => 'B',
                "right" => 'C',
                "left" => 'D',
                "home" => 'H',
                _ => 'F',
            };
            if modifiers != 1 {
                format!("\x1b[1;{modifiers}{code}")
            } else if application_cursor {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[{code}")
            }
        }
        "insert" | "delete" | "pageup" | "pagedown" | "f5" | "f6" | "f7" | "f8" | "f9" | "f10"
        | "f11" | "f12" => {
            let code = match key.key.as_str() {
                "insert" => 2,
                "delete" => 3,
                "pageup" => 5,
                "pagedown" => 6,
                "f5" => 15,
                "f6" => 17,
                "f7" => 18,
                "f8" => 19,
                "f9" => 20,
                "f10" => 21,
                "f11" => 23,
                _ => 24,
            };
            if modifiers == 1 {
                format!("\x1b[{code}~")
            } else {
                format!("\x1b[{code};{modifiers}~")
            }
        }
        "f1" | "f2" | "f3" | "f4" => {
            let code = match key.key.as_str() {
                "f1" => 'P',
                "f2" => 'Q',
                "f3" => 'R',
                _ => 'S',
            };
            if modifiers == 1 {
                format!("\x1bO{code}")
            } else {
                format!("\x1b[1;{modifiers}{code}")
            }
        }
        _ if key.modifiers.control => {
            let byte = match key.key.to_ascii_lowercase().as_str() {
                "space" | "@" | "2" => 0,
                "[" | "3" => 27,
                "\\" | "4" => 28,
                "]" | "5" => 29,
                "^" | "6" => 30,
                "_" | "-" | "7" => 31,
                "?" | "8" => 127,
                text if text.len() == 1 && text.as_bytes()[0].is_ascii_lowercase() => {
                    text.as_bytes()[0] - b'a' + 1
                }
                _ => return None,
            };
            let mut result = vec![byte];
            if key.modifiers.alt {
                result.insert(0, 27);
            }
            return Some(result);
        }
        _ => return None,
    };
    Some(sequence.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shell_editing_interrupts_and_application_keys_are_encoded() {
        for (key, expected) in [
            ("ctrl-c", b"\x03".as_slice()),
            ("ctrl-d", b"\x04".as_slice()),
            ("ctrl-r", b"\x12".as_slice()),
            ("tab", b"\t".as_slice()),
            ("up", b"\x1b[A".as_slice()),
            ("ctrl-left", b"\x1b[1;5D".as_slice()),
        ] {
            assert_eq!(
                key_bytes(&Keystroke::parse(key).unwrap(), false).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            key_bytes(&Keystroke::parse("up").unwrap(), true).unwrap(),
            b"\x1bOA"
        );
        assert!(key_bytes(&Keystroke::parse("a").unwrap(), false).is_none());
        assert!(key_bytes(&Keystroke::parse("cmd-v").unwrap(), false).is_none());
    }
}
