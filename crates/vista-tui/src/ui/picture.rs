//! Drawing the picture viewer: the picture on the whole screen, with a
//! title row (name, zoom, keys).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui_image::Image;

use crate::app::App;
use crate::picture::ViewFrame;
use crate::style::ThemeStyle;

pub fn draw(f: &mut Frame, area: Rect, app: &mut App) {
    let font = app.font_size();
    let App {
        picture, theme: t, tx, ..
    } = app;
    let Some(v) = picture.as_mut() else { return };
    f.render_widget(Clear, area);
    f.render_widget(Block::new().style(t.fg_bg("ui.text", "ui.bg")), area);
    if area.height < 3 {
        return;
    }
    let zoom = v.percent().map(|p| format!("  {p}%")).unwrap_or_default();
    let title = Line::from(vec![
        Span::styled(
            format!(" {}", v.title),
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ),
        Span::styled(zoom, t.fg("ui.text")),
        Span::styled(
            "   +/- or wheel zoom · arrows or drag move · 0 fit · Esc close",
            t.fg("ui.dim"),
        ),
    ]);
    f.render_widget(
        Paragraph::new(title).style(t.bg("ui.panel")),
        Rect { height: 1, ..area },
    );
    let body = Rect {
        y: area.y + 1,
        height: area.height - 1,
        ..area
    };
    v.view = body;
    v.font = font;
    let message = |f: &mut Frame, text: &str, token: &str| {
        let r = Rect {
            y: body.y + body.height / 2,
            height: 1,
            ..body
        };
        f.render_widget(
            Paragraph::new(Span::styled(text.to_string(), t.fg(token))).centered(),
            r,
        );
    };
    match &v.picture {
        None => message(f, "Loading…", "ui.dim"),
        Some(Err(e)) => message(f, &format!("Cannot show it: {e}"), "ui.error"),
        Some(Ok(_)) => {
            v.request_frame(tx);
            // The frame (the last one, while the next is drawn), centred.
            let size = match &v.frame {
                Some((_, ViewFrame::Kitty(k))) => Some((k.cols, k.rows)),
                Some((_, ViewFrame::Other(p))) => Some((p.size().width, p.size().height)),
                None => None,
            };
            if let Some((w, h)) = size {
                let (w, h) = (w.min(body.width), h.min(body.height));
                let r = Rect {
                    x: body.x + (body.width - w) / 2,
                    y: body.y + (body.height - h) / 2,
                    width: w,
                    height: h,
                };
                match &mut v.frame {
                    Some((_, ViewFrame::Kitty(k))) => k.render(r, f.buffer_mut()),
                    Some((_, ViewFrame::Other(p))) => f.render_widget(Image::new(p), r),
                    None => {}
                }
            }
        }
    }
}
