//! Mermaid diagrams (```` ```mermaid ```` blocks in Markdown), through
//! merman: Mermaid reimplemented in Rust, so no browser or Node.
//!
//! Two forms: Unicode box-drawing text (fast, for terminals without images
//! and while a picture is on its way) and a [`Diagram`]: the vector drawing,
//! for every diagram type, drawn to pixels at any size and zoom. The first
//! diagram takes about a second while system fonts load, so it's made on a
//! worker.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use merman::ascii::{AsciiRenderOptions, AsciiViewportPolicy, OverflowPolicy};
use merman::svg::SvgPipeline;
use merman::{AsciiRequest, OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use resvg::{tiny_skia, usvg};

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

/// A diagram's vector drawing, ready to draw any part of it at any size.
pub struct Diagram {
    tree: usvg::Tree,
}

/// The system's fonts, loaded once (the slow part of the first diagram).
fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone()
}

impl Diagram {
    /// Lays out `src` in Mermaid's dark or default theme. SVG made for resvg:
    /// labels as SVG text, not HTML.
    pub fn new(src: &str, dark: bool) -> Result<Diagram, String> {
        let config = merman::MermaidConfig::from_value(serde_json::json!({
            "theme": if dark { "dark" } else { "default" },
        }));
        let renderer = Renderer::new().with_engine(merman::Engine::new().with_site_config(config));
        let out = renderer.render(RenderRequest::svg(
            src,
            OperationControl::new(),
            SvgRequest {
                pipeline: Some(SvgPipeline::resvg_safe()),
                ..Default::default()
            },
        ));
        let svg = match out {
            Ok(RenderOutput::Svg(Some(svg))) => svg.svg().to_string(),
            Ok(_) => return Err("not a Mermaid diagram".to_string()),
            Err(e) => return Err(e.to_string()),
        };
        let opts = usvg::Options {
            fontdb: fonts(),
            ..Default::default()
        };
        let tree = usvg::Tree::from_str(&svg, &opts).map_err(|e| e.to_string())?;
        Ok(Diagram { tree })
    }

    /// Its natural size, in CSS pixels.
    pub fn size(&self) -> (f32, f32) {
        let s = self.tree.size();
        (s.width(), s.height())
    }

    /// Draws the part `region` (x, y, width, height in CSS pixels) into an
    /// image `out` pixels big (the region stretched to fill it), on
    /// `background` (RGB). Returns RGBA bytes, row by row.
    pub fn draw(&self, region: (f32, f32, f32, f32), out: (u32, u32), background: (u8, u8, u8)) -> Vec<u8> {
        let (w, h) = (out.0.max(1), out.1.max(1));
        let Some(mut pixmap) = tiny_skia::Pixmap::new(w, h) else {
            return Vec::new();
        };
        pixmap.fill(tiny_skia::Color::from_rgba8(
            background.0,
            background.1,
            background.2,
            255,
        ));
        let (sx, sy) = (w as f32 / region.2.max(0.001), h as f32 / region.3.max(0.001));
        let transform = tiny_skia::Transform::from_scale(sx, sy).pre_translate(-region.0, -region.1);
        resvg::render(&self.tree, transform, &mut pixmap.as_mut());
        // Opaque background: the premultiplied bytes are the plain ones.
        pixmap.take()
    }

    /// The whole diagram at `scale` times its natural size, at most `max`
    /// pixels a side: (width, height, RGBA bytes).
    pub fn draw_all(&self, scale: f32, max: u32, background: (u8, u8, u8)) -> (u32, u32, Vec<u8>) {
        let (w, h) = self.size();
        let scale = scale.min(max as f32 / w.max(1.0)).min(max as f32 / h.max(1.0));
        let out = ((w * scale).ceil() as u32, (h * scale).ceil() as u32);
        (out.0, out.1, self.draw((0.0, 0.0, w, h), out, background))
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
    fn draws_diagrams_as_pictures_of_any_part() {
        let d = Diagram::new("pie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 1\n", true).unwrap();
        let (w, h) = d.size();
        assert!(w > 100.0 && h > 100.0, "{w}×{h}");
        let (pw, ph, px) = d.draw_all(2.0, 4000, (43, 43, 43));
        assert_eq!((pw, ph), ((w * 2.0).ceil() as u32, (h * 2.0).ceil() as u32));
        assert_eq!(px.len(), (pw * ph * 4) as usize);
        assert_eq!(&px[..4], &[43, 43, 43, 255], "the corner is background");
        assert!(px.chunks(4).any(|p| p[..3] != [43, 43, 43]), "something is drawn");
        // Bounded: never more than `max` pixels a side.
        let (pw, ph, _) = d.draw_all(100.0, 500, (0, 0, 0));
        assert!(pw <= 500 && ph <= 500);
        // A region, stretched to the output size.
        let part = d.draw((w / 4.0, h / 4.0, w / 2.0, h / 2.0), (300, 200), (255, 255, 255));
        assert_eq!(part.len(), 300 * 200 * 4);
        assert!(Diagram::new("flowchart TD\n  A -->\n", false).is_err());
    }
}
