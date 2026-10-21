//! HTML traces of a test run, like Playwright's trace viewer: every step's
//! screen as coloured HTML, written when a test fails.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub struct Step {
    pub label: String,
    pub html: String,
}

fn css(c: vt100::Color, default: &str) -> String {
    match c {
        vt100::Color::Default => default.to_string(),
        vt100::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        vt100::Color::Idx(i) => format!("var(--c{i})"),
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Renders a screen as a `<pre>` with one `<span>` per style run.
pub fn screen_html(screen: &vt100::Screen) -> String {
    let (rows, cols) = screen.size();
    let mut out = String::from("<pre class=\"screen\">");
    for r in 0..rows.get() {
        let mut run_style = String::new();
        let mut run_text = String::new();
        let flush = |out: &mut String, style: &str, text: &str| {
            if !text.is_empty() {
                let _ = write!(out, "<span style=\"{style}\">{}</span>", escape(text));
            }
        };
        for c in 0..cols.get() {
            let Some(cell) = screen.cell(r, c) else { continue };
            if cell.is_wide_continuation() {
                continue;
            }
            let (mut fg, mut bg) = (css(cell.fgcolor(), "#cccccc"), css(cell.bgcolor(), "#111111"));
            if cell.inverse() {
                std::mem::swap(&mut fg, &mut bg);
            }
            let mut style = format!("color:{fg};background:{bg}");
            if cell.bold() {
                style.push_str(";font-weight:bold");
            }
            if cell.italic() {
                style.push_str(";font-style:italic");
            }
            if cell.underline() {
                style.push_str(";text-decoration:underline");
            }
            let text = if cell.contents().is_empty() {
                " "
            } else {
                cell.contents()
            };
            if style != run_style {
                flush(&mut out, &run_style, &run_text);
                run_style = style;
                run_text.clear();
            }
            run_text.push_str(text);
        }
        flush(&mut out, &run_style, &run_text);
        out.push('\n');
    }
    out.push_str("</pre>");
    out
}

pub fn write(dir: &Path, name: &str, steps: &[Step], extra: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let file = dir.join(format!("{}.html", sanitize(name)));
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>vista e2e trace</title><style>\
         body{background:#1b1b1b;color:#ddd;font:14px system-ui;margin:20px}\
         .step{margin:0 0 28px}.label{color:#ffc66d;margin:6px 0}\
         pre.screen{font:13px/1.25 'JetBrains Mono',Menlo,monospace;display:inline-block;margin:0;border:1px solid #444}\
         pre.extra{white-space:pre-wrap;color:#e07b6e}</style>",
    );
    let _ = write!(html, "<h1>{}</h1>", escape(name));
    if !extra.is_empty() {
        let _ = write!(html, "<pre class=\"extra\">{}</pre>", escape(extra));
    }
    for (i, s) in steps.iter().enumerate() {
        let _ = write!(
            html,
            "<div class=\"step\"><div class=\"label\">{}. {}</div>{}</div>",
            i + 1,
            escape(&s.label),
            s.html
        );
    }
    std::fs::write(&file, html)?;
    Ok(file)
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
