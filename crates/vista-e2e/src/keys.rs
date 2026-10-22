//! Turning key names (`ctrl+p`, `shift+f7`, `enter`) into the bytes a
//! terminal would send in legacy (xterm) encoding.

/// Encodes one key chord. Panics on names it doesn't know, so typos in tests
/// fail loudly.
pub fn encode(key: &str) -> Vec<u8> {
    // A single character is typed as is (`J` is Shift-J, not `j`).
    let mut chars = key.chars();
    if let (Some(c), None) = (chars.next(), chars.next())
        && !c.is_whitespace()
    {
        return c.to_string().into_bytes();
    }
    let lower = key.trim().to_ascii_lowercase();
    let mut parts: Vec<&str> = lower.split('+').collect();
    let name = if lower.ends_with("++") {
        parts.truncate(parts.len() - 2);
        "+".to_string()
    } else {
        parts.pop().unwrap_or_default().to_string()
    };
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    for m in &parts {
        match *m {
            "ctrl" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            other => panic!("unknown modifier `{other}` in `{key}`"),
        }
    }
    // xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4)
    let m = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(ctrl);
    let csi = |final_byte: char| -> Vec<u8> {
        if m == 1 {
            format!("\x1b[{final_byte}").into_bytes()
        } else {
            format!("\x1b[1;{m}{final_byte}").into_bytes()
        }
    };
    let tilde = |n: u8| -> Vec<u8> {
        if m == 1 {
            format!("\x1b[{n}~").into_bytes()
        } else {
            format!("\x1b[{n};{m}~").into_bytes()
        }
    };
    let bytes: Vec<u8> = match name.as_str() {
        "enter" | "return" => b"\r".to_vec(),
        "esc" | "escape" => b"\x1b".to_vec(),
        "tab" if shift => b"\x1b[Z".to_vec(),
        "tab" => b"\t".to_vec(),
        "backtab" => b"\x1b[Z".to_vec(),
        "backspace" => b"\x7f".to_vec(),
        "space" => b" ".to_vec(),
        "up" => csi('A'),
        "down" => csi('B'),
        "right" => csi('C'),
        "left" => csi('D'),
        "home" => csi('H'),
        "end" => csi('F'),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "delete" => tilde(3),
        "f1" => ss3_or_csi('P', m),
        "f2" => ss3_or_csi('Q', m),
        "f3" => ss3_or_csi('R', m),
        "f4" => ss3_or_csi('S', m),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        n if n.chars().count() == 1 => {
            let c = n.chars().next().unwrap_or(' ');
            let c = if shift && c.is_ascii_alphabetic() {
                c.to_ascii_uppercase()
            } else {
                c
            };
            let mut out = Vec::new();
            if alt {
                out.push(0x1b);
            }
            if ctrl {
                out.push(match c.to_ascii_lowercase() {
                    c @ 'a'..='z' => c as u8 - b'a' + 1,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    ' ' | '@' => 0,
                    other => panic!("can't encode ctrl+{other}"),
                });
            } else {
                let mut b = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
            }
            return out;
        }
        other => panic!("unknown key `{other}` in `{key}`"),
    };
    if alt
        && !matches!(name.as_str(), "up" | "down" | "left" | "right" | "home" | "end")
        && !bytes.starts_with(b"\x1b[")
    {
        let mut out = vec![0x1b];
        out.extend(bytes);
        return out;
    }
    bytes
}

fn ss3_or_csi(c: char, m: u8) -> Vec<u8> {
    if m == 1 {
        format!("\x1bO{c}").into_bytes()
    } else {
        format!("\x1b[1;{m}{c}").into_bytes()
    }
}

/// SGR (1006) mouse report. `button`: 0 left, 1 middle, 2 right, 64 wheel up,
/// 65 wheel down. Coordinates are 0-based cells.
pub fn mouse(button: u8, col: u16, row: u16, press: bool) -> Vec<u8> {
    let end = if press { 'M' } else { 'm' };
    format!("\x1b[<{button};{};{}{end}", col + 1, row + 1).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn encodes_common_keys() {
        assert_eq!(encode("ctrl+p"), vec![0x10]);
        assert_eq!(encode("J"), b"J".to_vec(), "single characters keep their case");
        assert_eq!(encode("]"), b"]".to_vec());
        assert_eq!(encode("ctrl+\\"), vec![0x1c]);
        assert_eq!(encode("alt+1"), b"\x1b1".to_vec());
        assert_eq!(encode("enter"), b"\r".to_vec());
        assert_eq!(encode("shift+tab"), b"\x1b[Z".to_vec());
        assert_eq!(encode("f7"), b"\x1b[18~".to_vec());
        assert_eq!(encode("shift+f7"), b"\x1b[18;2~".to_vec());
        assert_eq!(encode("ctrl+alt+down"), b"\x1b[1;7B".to_vec());
        assert_eq!(encode("alt+right"), b"\x1b[1;3C".to_vec());
        assert_eq!(encode("?"), b"?".to_vec());
        assert_eq!(encode("shift+a"), b"A".to_vec());
        assert_eq!(encode("space"), b" ".to_vec());
    }

    #[test]
    #[should_panic(expected = "unknown key")]
    fn rejects_unknown_keys() {
        encode("ctrl+banana");
    }
}
