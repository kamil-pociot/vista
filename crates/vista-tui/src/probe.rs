//! `vista --probe`: report what the terminal (and any multiplexer in between)
//! supports, so image and keyboard handling can be checked in Ghostty, herdr, …

use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, window_size};

/// What the terminal answered.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub kitty_graphics: bool,
    pub kitty_keyboard: bool,
    pub cell_pixels: Option<(u16, u16)>,
}

/// The kitty graphics support query: a 1×1 image that is only checked, never
/// stored, followed by a DA1 request so we know when the terminal is done.
pub const KITTY_GRAPHICS_QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[c";

/// Whether a terminal response confirms kitty graphics support.
pub fn graphics_ok(response: &[u8]) -> bool {
    let s = String::from_utf8_lossy(response);
    s.contains("_Gi=31;OK")
}

fn has_da1(response: &[u8]) -> bool {
    let s = String::from_utf8_lossy(response);
    s.find("\x1b[?").is_some_and(|i| s[i..].contains('c'))
}

/// Queries the terminal. Must run with stdin/stdout attached to it.
pub fn detect(timeout: Duration) -> Result<Capabilities> {
    enable_raw_mode()?;
    let kitty_keyboard = supports_keyboard_enhancement().unwrap_or(false);
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    thread::spawn(move || {
        let mut buf = [0u8; 256];
        let mut stdin = io::stdin();
        while let Ok(n) = stdin.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut out = io::stdout();
    out.write_all(KITTY_GRAPHICS_QUERY.as_bytes())?;
    out.flush()?;
    let start = Instant::now();
    let mut response = Vec::new();
    while start.elapsed() < timeout && !has_da1(&response) {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(20)) {
            response.extend(chunk);
        }
    }
    disable_raw_mode()?;
    let cell_pixels = window_size()
        .ok()
        .and_then(|w| (w.width > 0 && w.columns > 0 && w.rows > 0).then(|| (w.width / w.columns, w.height / w.rows)));
    Ok(Capabilities {
        kitty_graphics: graphics_ok(&response),
        kitty_keyboard,
        cell_pixels,
    })
}

/// Standard base64 (for the kitty graphics payload).
pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Kitty graphics escape sequences that transmit and display raw RGB pixels.
pub fn kitty_rgb_image(width: u32, height: u32, rgb: &[u8]) -> String {
    let payload = base64(rgb);
    let chunks: Vec<&str> = payload
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).expect("base64 is ascii"))
        .collect();
    let mut out = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            out.push_str(&format!(
                "\x1b_Ga=T,f=24,s={width},v={height},q=2,m={more};{chunk}\x1b\\"
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }
    }
    out
}

fn gradient(width: u32, height: u32) -> Vec<u8> {
    let mut px = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            px.push((x * 255 / width.max(1)) as u8);
            px.push((y * 255 / height.max(1)) as u8);
            px.push(180);
        }
    }
    px
}

pub fn run(show_image: bool) -> Result<()> {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| "-".into());
    println!("vista --probe");
    println!("  TERM            {}", env("TERM"));
    println!("  TERM_PROGRAM    {}", env("TERM_PROGRAM"));
    println!("  COLORTERM       {}", env("COLORTERM"));
    println!("  TMUX            {}", env("TMUX"));
    let herdr: Vec<String> = std::env::vars()
        .filter(|(k, _)| k.to_ascii_uppercase().contains("HERDR"))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    println!(
        "  herdr env       {}",
        if herdr.is_empty() { "-".into() } else { herdr.join(" ") }
    );
    let caps = detect(Duration::from_millis(1500))?;
    println!("  kitty graphics  {}", yes_no(caps.kitty_graphics));
    println!("  kitty keyboard  {}", yes_no(caps.kitty_keyboard));
    match caps.cell_pixels {
        Some((w, h)) => println!("  cell size       {w}×{h} px"),
        None => println!("  cell size       unknown (images fall back to an 8×16 estimate)"),
    }
    let mode = if caps.kitty_graphics { "kitty" } else { "halfblocks" };
    println!("  images would use: {mode}");
    if show_image {
        if caps.kitty_graphics {
            println!("\nTest image (a blue-to-pink gradient should appear below):");
            print!("{}", kitty_rgb_image(240, 80, &gradient(240, 80)));
            println!("\n\n\n\n\n");
        } else {
            println!("\nNo kitty graphics support detected; skipping the test image.");
        }
    }
    Ok(())
}

fn yes_no(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0, 0, 0]), "AAAA");
    }

    #[test]
    fn recognises_graphics_responses() {
        assert!(graphics_ok(b"\x1b_Gi=31;OK\x1b\\\x1b[?62;22c"));
        assert!(!graphics_ok(b"\x1b[?62;22c"));
        assert!(has_da1(b"\x1b[?1;2c"));
        assert!(!has_da1(b"\x1b_Gi=31;OK\x1b\\"));
    }

    #[test]
    fn chunks_large_images() {
        let seq = kitty_rgb_image(100, 100, &vec![7u8; 30_000]);
        assert!(seq.starts_with("\x1b_Ga=T,f=24,s=100,v=100,q=2,m=1;"));
        assert!(seq.ends_with("\x1b\\"));
        assert!(seq.contains("\x1b_Gm=0;"));
        assert_eq!(seq.matches("\x1b_G").count(), 40_000usize.div_ceil(4096));
    }
}
