use alacritty_terminal::term::TermMode;
use gpui::Keystroke;

fn modifier_code(keystroke: &Keystroke) -> u8 {
    let m = &keystroke.modifiers;
    1 + m.shift as u8 + (m.alt as u8) * 2 + (m.control as u8) * 4 + (m.platform as u8) * 8
}

fn csi_modified(code: &str, final_char: char, modifiers: u8) -> Vec<u8> {
    if modifiers > 1 {
        format!("\x1b[{code};{modifiers}{final_char}").into_bytes()
    } else {
        format!("\x1b[{code}{final_char}").into_bytes()
    }
}

fn cursor_key(letter: char, modifiers: u8, mode: TermMode) -> Vec<u8> {
    if modifiers > 1 {
        format!("\x1b[1;{modifiers}{letter}").into_bytes()
    } else if mode.contains(TermMode::APP_CURSOR) {
        format!("\x1bO{letter}").into_bytes()
    } else {
        format!("\x1b[{letter}").into_bytes()
    }
}

fn tilde(number: u8, modifiers: u8) -> Vec<u8> {
    if modifiers > 1 {
        format!("\x1b[{number};{modifiers}~").into_bytes()
    } else {
        format!("\x1b[{number}~").into_bytes()
    }
}

pub fn mac_shortcut(keystroke: &Keystroke) -> Option<&'static [u8]> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let m = &keystroke.modifiers;
    match (keystroke.key.as_str(), m.platform, m.alt, m.control, m.shift) {
        ("left", true, false, false, false) => Some(b"\x01"),
        ("right", true, false, false, false) => Some(b"\x05"),
        ("backspace", true, false, false, false) => Some(b"\x15"),
        ("left", false, true, false, false) => Some(b"\x1bb"),
        ("right", false, true, false, false) => Some(b"\x1bf"),
        ("backspace", false, true, false, false) => Some(b"\x17"),
        _ => None,
    }
}

pub const CLIPBOARD_KEYS: bool = cfg!(windows);

pub fn plain_control(keystroke: &Keystroke, key: &str) -> bool {
    let m = &keystroke.modifiers;
    keystroke.key == key && m.control && !m.shift && !m.alt && !m.platform && !m.function
}

pub fn encode(keystroke: &Keystroke, mode: TermMode, option_as_meta: bool) -> Option<Vec<u8>> {
    if let Some(bytes) = mac_shortcut(keystroke) {
        return Some(bytes.to_vec());
    }
    let m = &keystroke.modifiers;
    if m.platform {
        return None;
    }
    let modifiers = modifier_code(keystroke);
    let kitty = mode.contains(TermMode::DISAMBIGUATE_ESC_CODES);
    let key = keystroke.key.as_str();
    let special = match key {
        "enter" => Some((13u32, if m.shift && !m.control && !m.alt { b"\n".to_vec() } else { b"\r".to_vec() })),
        "tab" => Some((9, if m.shift && !m.control && !m.alt { b"\x1b[Z".to_vec() } else { b"\t".to_vec() })),
        "backspace" => Some((127, if m.control { b"\x08".to_vec() } else { b"\x7f".to_vec() })),
        "escape" => Some((27, b"\x1b".to_vec())),
        _ => None,
    };
    if let Some((code, legacy)) = special {
        if kitty && (modifiers > 1 || code == 27) {
            return Some(
                if modifiers > 1 { format!("\x1b[{code};{modifiers}u") } else { format!("\x1b[{code}u") }.into_bytes(),
            );
        }
        if key == "tab" && m.shift {
            return Some(legacy);
        }
        return Some(if m.alt { [b"\x1b".as_slice(), &legacy].concat() } else { legacy });
    }
    let sequence = match key {
        "up" => Some(cursor_key('A', modifiers, mode)),
        "down" => Some(cursor_key('B', modifiers, mode)),
        "right" => Some(cursor_key('C', modifiers, mode)),
        "left" => Some(cursor_key('D', modifiers, mode)),
        "home" => Some(cursor_key('H', modifiers, mode)),
        "end" => Some(cursor_key('F', modifiers, mode)),
        "pageup" => Some(tilde(5, modifiers)),
        "pagedown" => Some(tilde(6, modifiers)),
        "insert" => Some(tilde(2, modifiers)),
        "delete" => Some(tilde(3, modifiers)),
        "f1" => Some(if modifiers > 1 { csi_modified("1", 'P', modifiers) } else { b"\x1bOP".to_vec() }),
        "f2" => Some(if modifiers > 1 { csi_modified("1", 'Q', modifiers) } else { b"\x1bOQ".to_vec() }),
        "f3" => Some(if modifiers > 1 { csi_modified("1", 'R', modifiers) } else { b"\x1bOR".to_vec() }),
        "f4" => Some(if modifiers > 1 { csi_modified("1", 'S', modifiers) } else { b"\x1bOS".to_vec() }),
        "f5" => Some(tilde(15, modifiers)),
        "f6" => Some(tilde(17, modifiers)),
        "f7" => Some(tilde(18, modifiers)),
        "f8" => Some(tilde(19, modifiers)),
        "f9" => Some(tilde(20, modifiers)),
        "f10" => Some(tilde(21, modifiers)),
        "f11" => Some(tilde(23, modifiers)),
        "f12" => Some(tilde(24, modifiers)),
        _ => None,
    };
    if sequence.is_some() {
        return sequence;
    }
    let base = if key == "space" { ' ' } else { key.chars().next().filter(|_| key.chars().count() == 1)? };
    if m.control && !(cfg!(windows) && m.alt && keystroke.key_char.is_some()) {
        if kitty && (m.alt || m.shift) && base.is_ascii_alphanumeric() {
            return Some(format!("\x1b[{};{modifiers}u", base.to_ascii_lowercase() as u32).into_bytes());
        }
        let byte = control_byte(base)?;
        return Some(if m.alt { vec![0x1b, byte] } else { vec![byte] });
    }
    if m.alt && (option_as_meta || !cfg!(target_os = "macos")) {
        let text = if key == "space" {
            " ".to_string()
        } else if m.shift {
            keystroke
                .key_char
                .clone()
                .filter(|text| text.chars().count() == 1 && text.is_ascii())
                .unwrap_or_else(|| base.to_uppercase().collect())
        } else {
            base.to_string()
        };
        return Some([b"\x1b".as_slice(), text.as_bytes()].concat());
    }
    None
}

fn control_byte(key: char) -> Option<u8> {
    match key {
        'a'..='z' => Some(key as u8 - b'a' + 1),
        'A'..='Z' => Some(key as u8 - b'A' + 1),
        ' ' | '@' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '/' | '7' => Some(0x1f),
        '8' | '?' => Some(0x7f),
        _ => None,
    }
}

pub fn paste(text: &str, mode: TermMode) -> Vec<u8> {
    let normalized = text.replace("\r\n", "\r").replace('\n', "\r");
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let clean = normalized.replace('\x1b', "");
        format!("\x1b[200~{clean}\x1b[201~").into_bytes()
    } else {
        normalized.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Keystroke {
        Keystroke::parse(text).unwrap()
    }

    #[test]
    fn encodes_keys() {
        let mode = TermMode::default();
        assert_eq!(encode(&key("enter"), mode, true), Some(b"\r".to_vec()));
        assert_eq!(encode(&key("shift-enter"), mode, true), Some(b"\n".to_vec()));
        assert_eq!(encode(&key("ctrl-c"), mode, true), Some(vec![3]));
        assert_eq!(encode(&key("up"), mode, true), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(&key("up"), TermMode::APP_CURSOR, true), Some(b"\x1bOA".to_vec()));
        assert_eq!(encode(&key("shift-up"), mode, true), Some(b"\x1b[1;2A".to_vec()));
        assert_eq!(encode(&key("shift-tab"), mode, true), Some(b"\x1b[Z".to_vec()));
        assert_eq!(encode(&key("alt-b"), mode, true), Some(b"\x1bb".to_vec()));
        assert_eq!(encode(&key("shift-enter"), TermMode::DISAMBIGUATE_ESC_CODES, true), Some(b"\x1b[13;2u".to_vec()));
        assert_eq!(encode(&key("escape"), TermMode::DISAMBIGUATE_ESC_CODES, true), Some(b"\x1b[27u".to_vec()));
        assert_eq!(encode(&key("a"), mode, true), None);
    }

    #[test]
    fn pastes() {
        assert_eq!(paste("a\nb", TermMode::default()), b"a\rb".to_vec());
        assert_eq!(paste("x\x1b", TermMode::BRACKETED_PASTE), b"\x1b[200~x\x1b[201~".to_vec());
    }

    #[test]
    fn spots_plain_control_keys() {
        assert!(plain_control(&key("ctrl-c"), "c"));
        assert!(!plain_control(&key("ctrl-shift-c"), "c"));
        assert!(!plain_control(&key("ctrl-alt-c"), "c"));
        assert!(!plain_control(&key("c"), "c"));
    }
}
