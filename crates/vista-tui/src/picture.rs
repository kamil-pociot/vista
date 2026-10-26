//! The picture viewer: an image or Mermaid diagram from Markdown on the
//! whole screen, zoomed and moved around like in an image viewer. Clicking
//! a picture opens it.
//!
//! Each view is drawn on a worker: a diagram from its vectors (sharp at any
//! zoom), an image cropped and scaled. The last frame stays up meanwhile.

use std::path::PathBuf;
use std::sync::Arc;

use image::DynamicImage;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use vista_render::mermaid::Diagram;

use crate::event::{AppEvent, EventTx};

/// Each zoom step, in or out.
const STEP: f64 = 1.25;
/// A pan step: this share of what's in view.
const PAN: f64 = 0.15;
/// The closest zoom: 32 screen pixels per picture unit (an image's pixel
/// stays a crisp square; a diagram is drawn from its vectors).
const MAX_SCALE: f64 = 32.0;

/// What's shown.
#[derive(Debug, Clone)]
pub enum Source {
    File(PathBuf),
    Diagram {
        src: String,
        dark: bool,
        background: vista_theme::Rgb,
    },
}

/// A source, loaded.
pub enum Picture {
    Bitmap(DynamicImage),
    Diagram(Box<Diagram>, vista_theme::Rgb),
}

impl Picture {
    /// Size in its own units: pixels, or a diagram's CSS pixels.
    pub fn size(&self) -> (f64, f64) {
        match self {
            Picture::Bitmap(img) => (img.width() as f64, img.height() as f64),
            Picture::Diagram(d, _) => {
                let (w, h) = d.size();
                (w as f64, h as f64)
            }
        }
    }

    /// Draws `region` (x, y, w, h in its units) at `out` pixels.
    fn draw(&self, region: (f64, f64, f64, f64), out: (u32, u32)) -> DynamicImage {
        match self {
            Picture::Bitmap(img) => {
                let (x, y) = (region.0.floor().max(0.0) as u32, region.1.floor().max(0.0) as u32);
                let w = (region.2.ceil() as u32).clamp(1, img.width().saturating_sub(x).max(1));
                let h = (region.3.ceil() as u32).clamp(1, img.height().saturating_sub(y).max(1));
                // Enlarged pixels stay crisp; reduced ones are smoothed.
                let filter = if out.0 > w * 2 {
                    image::imageops::FilterType::Nearest
                } else {
                    image::imageops::FilterType::Triangle
                };
                img.crop_imm(x, y, w, h)
                    .resize_exact(out.0.max(1), out.1.max(1), filter)
            }
            Picture::Diagram(d, bg) => {
                let r = (region.0 as f32, region.1 as f32, region.2 as f32, region.3 as f32);
                let px = d.draw(r, out, (bg.0, bg.1, bg.2));
                image::RgbaImage::from_raw(out.0.max(1), out.1.max(1), px)
                    .map(DynamicImage::ImageRgba8)
                    .unwrap_or_default()
            }
        }
    }
}

/// What a frame shows: zoom, centre and view size (frames for other views
/// are stale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameKey {
    pub zoom: f64,
    pub center: (f64, f64),
    pub view_px: (u32, u32),
}

/// Results from the viewer's workers.
pub enum PictureEvent {
    Loaded {
        generation: u64,
        picture: Result<Arc<Picture>, String>,
    },
    Frame {
        generation: u64,
        key: FrameKey,
        image: Box<DynamicImage>,
    },
}

/// The part of a `size` picture in a `view` (pixels) at `zoom` (1: the whole
/// picture fits) around `center` (a fraction of the picture): the region in
/// its units, and the pixels it's drawn at. Never past the picture's edges.
pub fn region(size: (f64, f64), view: (f64, f64), zoom: f64, center: (f64, f64)) -> ((f64, f64, f64, f64), (u32, u32)) {
    let (sw, sh) = (size.0.max(1.0), size.1.max(1.0));
    let scale = fit(size, view) * zoom;
    let (rw, rh) = ((view.0 / scale).min(sw), (view.1 / scale).min(sh));
    let cx = (center.0 * sw).clamp(rw / 2.0, sw - rw / 2.0);
    let cy = (center.1 * sh).clamp(rh / 2.0, sh - rh / 2.0);
    let out = (
        ((rw * scale).round() as u32).max(1),
        ((rh * scale).round() as u32).max(1),
    );
    ((cx - rw / 2.0, cy - rh / 2.0, rw, rh), out)
}

/// Screen pixels per picture unit when the whole picture just fits.
fn fit(size: (f64, f64), view: (f64, f64)) -> f64 {
    (view.0 / size.0.max(1.0)).min(view.1 / size.1.max(1.0)).max(1e-6)
}

pub struct PictureView {
    /// Shown at the top: the image's file name, or "Mermaid diagram".
    pub title: String,
    pub source: Source,
    /// `None` while loading.
    pub picture: Option<Result<Arc<Picture>, String>>,
    /// 1: the whole picture fits the screen.
    pub zoom: f64,
    /// The point in the middle of the view, as a fraction of the picture.
    pub center: (f64, f64),
    /// Where the picture is drawn and the terminal's cell size in pixels
    /// (from the last draw; the mouse needs both).
    pub view: Rect,
    pub font: (u16, u16),
    /// The frame on screen, encoded for the terminal.
    pub frame: Option<(FrameKey, ratatui_image::protocol::Protocol)>,
    /// A frame is being drawn on a worker; another view was asked for meanwhile.
    pub busy: bool,
    pub generation: u64,
    /// Where a drag was last (cells).
    drag: Option<(u16, u16)>,
}

impl PictureView {
    pub fn new(title: String, source: Source) -> PictureView {
        PictureView {
            title,
            source,
            picture: None,
            zoom: 1.0,
            center: (0.5, 0.5),
            view: Rect::default(),
            font: (8, 16),
            frame: None,
            busy: false,
            generation: 0,
            drag: None,
        }
    }

    /// Loads the source on a worker.
    pub fn load(&mut self, tx: &EventTx) {
        self.generation += 1;
        let (generation, source, tx) = (self.generation, self.source.clone(), tx.clone());
        std::thread::spawn(move || {
            let picture = match source {
                Source::File(path) => image::open(&path).map(Picture::Bitmap).map_err(|e| e.to_string()),
                Source::Diagram { src, dark, background } => {
                    Diagram::new(&src, dark).map(|d| Picture::Diagram(Box::new(d), background))
                }
            }
            .map(Arc::new);
            let _ = tx.send(AppEvent::Picture(PictureEvent::Loaded { generation, picture }));
        });
    }

    fn loaded(&self) -> Option<&Arc<Picture>> {
        self.picture.as_ref().and_then(|p| p.as_ref().ok())
    }

    fn view_px(&self) -> (u32, u32) {
        (
            self.view.width as u32 * self.font.0 as u32,
            self.view.height as u32 * self.font.1 as u32,
        )
    }

    /// The view the next frame should show.
    pub fn key(&self) -> FrameKey {
        FrameKey {
            zoom: self.zoom,
            center: self.center,
            view_px: self.view_px(),
        }
    }

    /// Asks for a frame of the current view unless one is on screen or on
    /// its way (that one asks again when it's in, if the view moved on).
    pub fn request_frame(&mut self, tx: &EventTx) {
        let key = self.key();
        if self.busy || self.frame.as_ref().is_some_and(|(k, _)| *k == key) || key.view_px.0 == 0 {
            return;
        }
        let Some(picture) = self.loaded().cloned() else { return };
        self.busy = true;
        let (generation, tx) = (self.generation, tx.clone());
        std::thread::spawn(move || {
            let view = (key.view_px.0 as f64, key.view_px.1 as f64);
            let (r, out) = region(picture.size(), view, key.zoom, key.center);
            let image = Box::new(picture.draw(r, out));
            let _ = tx.send(AppEvent::Picture(PictureEvent::Frame { generation, key, image }));
        });
    }

    /// Keeps the centre where the view stays on the picture.
    fn settle(&mut self) {
        let Some(p) = self.loaded() else { return };
        let (size, max) = (p.size(), MAX_SCALE);
        let view = self.view_px();
        let view = (view.0.max(1) as f64, view.1.max(1) as f64);
        // No closer than the picture allows; never smaller than fitting.
        self.zoom = self.zoom.clamp(1.0, (max / fit(size, view)).max(1.0));
        let ((x, y, w, h), _) = region(size, view, self.zoom, self.center);
        self.center = ((x + w / 2.0) / size.0.max(1.0), (y + h / 2.0) / size.1.max(1.0));
    }

    /// Zooms by `factor`, keeping the picture's point at `at` (pixels from
    /// the view's top left) where it is; `None`: the middle.
    pub fn zoom_by(&mut self, factor: f64, at: Option<(f64, f64)>) {
        let Some(p) = self.loaded() else { return };
        let size = p.size();
        let view = self.view_px();
        let view = (view.0.max(1) as f64, view.1.max(1) as f64);
        let at = at.unwrap_or((view.0 / 2.0, view.1 / 2.0));
        let ((x, y, _, _), _) = region(size, view, self.zoom, self.center);
        let before = fit(size, view) * self.zoom;
        // The picture's point under `at` (in its units).
        let point = (x + at.0 / before, y + at.1 / before);
        let old = self.zoom;
        self.zoom *= factor;
        self.settle();
        if self.zoom == old {
            return;
        }
        let after = fit(size, view) * self.zoom;
        let center = (
            point.0 - at.0 / after + view.0 / after / 2.0,
            point.1 - at.1 / after + view.1 / after / 2.0,
        );
        self.center = (center.0 / size.0.max(1.0), center.1 / size.1.max(1.0));
        self.settle();
    }

    /// Moves the view by `delta` screen pixels.
    pub fn pan_px(&mut self, delta: (f64, f64)) {
        let Some(p) = self.loaded() else { return };
        let size = p.size();
        let view = self.view_px();
        let scale = fit(size, (view.0.max(1) as f64, view.1.max(1) as f64)) * self.zoom;
        self.center.0 += delta.0 / scale / size.0.max(1.0);
        self.center.1 += delta.1 / scale / size.1.max(1.0);
        self.settle();
    }

    /// Moves by a share of the view (arrow keys).
    fn pan_step(&mut self, dx: f64, dy: f64) {
        let view = self.view_px();
        self.pan_px((dx * PAN * view.0 as f64, dy * PAN * view.1 as f64));
    }

    /// The zoom shown at the top, in percent of the picture's own size
    /// (an image's pixels, a diagram's natural size).
    pub fn percent(&self) -> Option<u32> {
        let p = self.loaded()?;
        let view = self.view_px();
        let fit = fit(p.size(), (view.0.max(1) as f64, view.1.max(1) as f64));
        // On screen, one picture unit is `fit × zoom` screen pixels.
        Some((fit * self.zoom * 100.0).round() as u32)
    }

    /// Keys; false closes the viewer.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return false,
            KeyCode::Char('+') | KeyCode::Char('=') => self.zoom_by(STEP, None),
            KeyCode::Char('-') | KeyCode::Char('_') => self.zoom_by(1.0 / STEP, None),
            KeyCode::Char('0') => {
                self.zoom = 1.0;
                self.center = (0.5, 0.5);
            }
            KeyCode::Left | KeyCode::Char('h') => self.pan_step(-1.0, 0.0),
            KeyCode::Right | KeyCode::Char('l') => self.pan_step(1.0, 0.0),
            KeyCode::Up | KeyCode::Char('k') => self.pan_step(0.0, -1.0),
            KeyCode::Down | KeyCode::Char('j') => self.pan_step(0.0, 1.0),
            KeyCode::Char('c') if ctrl => return false,
            _ => {}
        }
        true
    }

    /// The wheel zooms around the mouse, dragging moves the picture,
    /// sideways scrolling moves it too.
    pub fn handle_mouse(&mut self, m: &MouseEvent) {
        let at = || {
            Some((
                (m.column.saturating_sub(self.view.x)) as f64 * self.font.0 as f64,
                (m.row.saturating_sub(self.view.y)) as f64 * self.font.1 as f64,
            ))
        };
        match m.kind {
            MouseEventKind::ScrollUp => self.zoom_by(STEP, at()),
            MouseEventKind::ScrollDown => self.zoom_by(1.0 / STEP, at()),
            MouseEventKind::ScrollLeft => self.pan_step(-0.5, 0.0),
            MouseEventKind::ScrollRight => self.pan_step(0.5, 0.0),
            MouseEventKind::Down(MouseButton::Left) => self.drag = Some((m.column, m.row)),
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((c, r)) = self.drag {
                    let dx = (c as f64 - m.column as f64) * self.font.0 as f64;
                    let dy = (r as f64 - m.row as f64) * self.font.1 as f64;
                    self.pan_px((dx, dy));
                }
                self.drag = Some((m.column, m.row));
            }
            MouseEventKind::Up(_) => self.drag = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_of(size: (u32, u32)) -> PictureView {
        let mut v = PictureView::new("t".into(), Source::File(PathBuf::from("x.png")));
        v.picture = Some(Ok(Arc::new(Picture::Bitmap(DynamicImage::new_rgba8(size.0, size.1)))));
        // 100×50 cells of 10×20 pixels: a 1000×1000 pixel view.
        v.view = Rect::new(0, 1, 100, 50);
        v.font = (10, 20);
        v
    }

    #[test]
    fn the_whole_picture_fits_at_zoom_one() {
        let (r, out) = region((400.0, 200.0), (1000.0, 1000.0), 1.0, (0.5, 0.5));
        assert_eq!(r, (0.0, 0.0, 400.0, 200.0));
        assert_eq!(out, (1000, 500), "stretched to the view's width");
        // Zoomed in twice: half of it in each direction, around the centre.
        let (r, out) = region((400.0, 200.0), (1000.0, 1000.0), 2.0, (0.5, 0.5));
        assert_eq!(r, (100.0, 0.0, 200.0, 200.0), "all of its height still fits");
        assert_eq!(out, (1000, 1000));
        // The centre never takes the view past an edge.
        let (r, _) = region((400.0, 200.0), (1000.0, 1000.0), 2.0, (0.0, 0.0));
        assert_eq!((r.0, r.1), (0.0, 0.0));
        let (r, _) = region((400.0, 200.0), (1000.0, 1000.0), 2.0, (1.0, 1.0));
        assert_eq!(r.0 + r.2, 400.0);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_mouse() {
        let mut v = view_of((1000, 1000));
        assert_eq!(v.percent(), Some(100));
        // The top-left quarter's middle stays put when zooming at it.
        v.zoom_by(2.0, Some((250.0, 250.0)));
        assert_eq!(v.zoom, 2.0);
        let ((x, y, w, h), _) = region((1000.0, 1000.0), (1000.0, 1000.0), v.zoom, v.center);
        assert_eq!((x, y, w, h), (125.0, 125.0, 500.0, 500.0));
        assert_eq!(v.percent(), Some(200));
        // Out past fitting: back to the whole picture, centred.
        v.zoom_by(0.1, None);
        assert_eq!(v.zoom, 1.0);
        assert_eq!(v.center, (0.5, 0.5));
    }

    #[test]
    fn zoom_has_limits() {
        // 32 screen pixels per picture pixel at most: a 100-pixel image fits
        // a 1000-pixel view at 10×, so it zooms 3.2 times further.
        let mut v = view_of((100, 100));
        for _ in 0..40 {
            v.zoom_by(STEP, None);
        }
        assert_eq!(v.percent(), Some(3200));
        assert!((v.zoom - 3.2).abs() < 1e-9);
        // A picture far bigger than the screen can't be made smaller than fitting.
        let mut v = view_of((4000, 4000));
        v.zoom_by(0.5, None);
        assert_eq!((v.zoom, v.percent()), (1.0, Some(25)));
    }

    #[test]
    fn keys_and_dragging_move_the_picture() {
        let mut v = view_of((1000, 1000));
        let center = v.center;
        v.handle_key(&KeyEvent::from(KeyCode::Right));
        assert_eq!(v.center, center, "nothing to move to when it all fits");
        v.handle_key(&KeyEvent::from(KeyCode::Char('+')));
        v.handle_key(&KeyEvent::from(KeyCode::Char('+')));
        assert!((v.zoom - 1.5625).abs() < 1e-9);
        v.handle_key(&KeyEvent::from(KeyCode::Right));
        assert!(v.center.0 > 0.5 && v.center.1 == 0.5, "{:?}", v.center);
        // Dragging left by 10 cells moves the view right by 100 screen pixels.
        let before = v.center;
        let ev = |kind, column| MouseEvent {
            kind,
            column,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        v.handle_mouse(&ev(MouseEventKind::Down(MouseButton::Left), 50));
        v.handle_mouse(&ev(MouseEventKind::Drag(MouseButton::Left), 40));
        let moved = (v.center.0 - before.0) * 1000.0 * v.zoom;
        assert!((moved - 100.0).abs() < 1e-6, "{moved}");
        v.handle_key(&KeyEvent::from(KeyCode::Char('0')));
        assert_eq!((v.zoom, v.center), (1.0, (0.5, 0.5)));
        assert!(!v.handle_key(&KeyEvent::from(KeyCode::Esc)), "Esc closes");
    }

    #[test]
    fn frames_crop_and_scale_images() {
        let mut img = image::RgbaImage::new(4, 2);
        img.put_pixel(3, 1, image::Rgba([255, 0, 0, 255]));
        let p = Picture::Bitmap(DynamicImage::ImageRgba8(img));
        // The bottom-right pixel, blown up: crisp, all red.
        let out = p.draw((3.0, 1.0, 1.0, 1.0), (16, 16)).to_rgba8();
        assert_eq!(out.dimensions(), (16, 16));
        assert!(out.pixels().all(|px| *px == image::Rgba([255, 0, 0, 255])));
    }
}
