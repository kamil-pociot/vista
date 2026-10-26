//! Mermaid diagrams (```` ```mermaid ```` blocks in Markdown), through
//! merman: Mermaid reimplemented in Rust, so no browser or Node.
//!
//! Two forms: Unicode box-drawing text (fast, for terminals without images
//! and while a picture is on its way) and PNG (every diagram type; the first
//! one takes about a second while fonts load, so it's made on a worker).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};

use merman::ascii::{AsciiRenderOptions, AsciiViewportPolicy, OverflowPolicy};
use merman::svg::export::{RasterFitBox, RasterOptions};
use merman::{AsciiRequest, OperationControl, PngRequest, RenderOutput, RenderRequest, Renderer, SvgRequest};

/// Whether a fenced block's info string (` ```mermaid `) is a diagram.
pub fn is_mermaid(lang: &str) -> bool {
    lang.split_whitespace().next() == Some("mermaid")
}

/// What a diagram is known by in the Markdown image cache: `mermaid:<hash>`.
pub fn key(src: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    format!("mermaid:{:016x}", h.finish())
}

/// Text renderings so far, by (source, width): Markdown re-renders on every
/// resize and reload.
type TextCache = HashMap<(String, usize), Result<Vec<String>, String>>;

fn text_cache() -> &'static Mutex<TextCache> {
    static CACHE: OnceLock<Mutex<TextCache>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The diagram drawn with Unicode box-drawing characters, at most `width`
/// columns where it can be (wider ones scroll sideways). An error for
/// diagram types or shapes merman can't draw as text (pie charts, some
/// crossing routes) and for invalid sources.
pub fn text(src: &str, width: usize) -> Result<Vec<String>, String> {
    let k = (src.to_string(), width);
    if let Some(hit) = text_cache().lock().ok().and_then(|c| c.get(&k).cloned()) {
        return hit;
    }
    let out = Renderer::new().render(RenderRequest::ascii(
        src,
        OperationControl::new(),
        AsciiRequest {
            options: AsciiRenderOptions::unicode(),
            viewport: AsciiViewportPolicy::with_max_width(width.max(20)).overflow(OverflowPolicy::Fallback),
            ..Default::default()
        },
    ));
    let res = match out {
        Ok(RenderOutput::Ascii(Some(report))) => Ok(report.text.lines().map(|l| l.trim_end().to_string()).collect()),
        Ok(_) => Err("not a Mermaid diagram".to_string()),
        Err(e) => Err(e.to_string()),
    };
    if let Ok(mut c) = text_cache().lock() {
        if c.len() > 256 {
            c.clear();
        }
        c.insert(k, res.clone());
    }
    res
}

/// The diagram as a PNG, in Mermaid's dark or default theme, on
/// `background` (a CSS colour such as `#2b2b2b`). Slow the first time.
pub fn png(src: &str, dark: bool, background: &str) -> Result<Vec<u8>, String> {
    let config = merman::MermaidConfig::from_value(serde_json::json!({
        "theme": if dark { "dark" } else { "default" },
    }));
    let renderer = Renderer::new().with_engine(merman::Engine::new().with_site_config(config));
    let out = renderer.render(RenderRequest::png(
        src,
        OperationControl::new(),
        PngRequest {
            svg: SvgRequest::default(),
            // Sharp in the terminal; bounded so a huge diagram can't take all memory.
            options: RasterOptions::default()
                .with_fit_to(RasterFitBox::contain(2400, 2400))
                .with_scale(2.0)
                .with_background(background),
        },
    ));
    match out {
        Ok(RenderOutput::Png(Some(p))) => Ok(p.bytes),
        Ok(_) => Err("not a Mermaid diagram".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEQUENCE: &str = "sequenceDiagram\n  User->>vista: open README.md\n  vista-->>User: diagram\n";

    #[test]
    fn recognises_mermaid_fences() {
        assert!(is_mermaid("mermaid"));
        assert!(is_mermaid("mermaid  {title=x}"));
        assert!(!is_mermaid("mermaidx"));
        assert!(!is_mermaid("rust"));
        assert_eq!(key(SEQUENCE), key(SEQUENCE));
        assert_ne!(key(SEQUENCE), key("flowchart LR\n  A --> B\n"));
        assert!(key(SEQUENCE).starts_with("mermaid:"));
    }

    #[test]
    fn draws_diagrams_as_text() {
        let lines = text(SEQUENCE, 60).unwrap();
        let all = lines.join("\n");
        assert!(all.contains("open README.md"), "{all}");
        assert!(all.contains('┌') && all.contains('►'), "{all}");
        let flow = text("flowchart LR\n  A[Tree] --> B[Viewer]\n", 60).unwrap().join("\n");
        assert!(flow.contains("│ Tree ├") && flow.contains("Viewer"), "{flow}");
        // Cached: the same answer again.
        assert_eq!(text(SEQUENCE, 60).unwrap(), lines);
    }

    #[test]
    fn says_why_a_diagram_has_no_text_form() {
        assert!(
            text("pie title Pets\n  \"Dogs\" : 3\n", 60)
                .unwrap_err()
                .contains("pie")
        );
        assert!(text("flowchart TD\n  A -->\n", 60).is_err(), "a parse error");
    }

    #[test]
    fn draws_diagrams_as_png() {
        let bytes = png("pie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 1\n", true, "#2b2b2b").unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert!(png("flowchart TD\n  A -->\n", false, "white").is_err());
    }
}
