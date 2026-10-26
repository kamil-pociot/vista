# 26 · Picture viewer

Click a picture (an image or a [Mermaid diagram](25-mermaid-diagrams.md) in rendered Markdown, or the image in an image file's tab), or press `v`, and it opens on the whole screen, to zoom in and move around as in an image viewer.

```
 Mermaid diagram  266%   +/- or wheel zoom · arrows or drag move · 0 fit · Esc close
┌──────────────────────────────────────────────────────────────────────────────┐
│                                                                              │
│                      (the picture, zoomed, centred)                          │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

## What it does

- Opens fitted to the screen (a small picture is enlarged to fill it). The top row shows its name and the zoom, in percent of its own size (an image's pixels, a diagram's natural size).
- **Zoom** with `+` / `-` (25% a step) or the mouse wheel, which keeps the point under the mouse where it is. From fitting the screen up to 32 screen pixels per picture pixel; never smaller than fitting.
- **Move around** with the arrow keys or `h` `j` `k` `l`, by dragging with the mouse, or scrolling sideways. The view stops at the picture's edges.
- `0` fits it again; `Esc` or `q` closes it.
- **Diagrams stay sharp at any zoom**: each view is drawn from the diagram's vectors at the screen's resolution, not enlarged from a picture. Images are cropped and scaled: enlarged pixels stay crisp squares, reduced ones are smoothed.
- Each view is drawn on a worker (a few milliseconds); the last one stays up meanwhile.
- `v` opens Markdown's first picture in view. With images off (`[images] protocol = "off"`) there's nothing to draw it with, and the status bar says so.

Pictures in Markdown also scroll like text now: one half out of view shows the part in view, at the top or the bottom ([03](03-markdown-and-images.md)).

## Keys

| Key | Action |
|---|---|
| `v`, click | open (in the viewer: rendered Markdown, an image tab) |
| `+` / `=`, wheel up | zoom in |
| `-`, wheel down | zoom out |
| arrows, `h` `j` `k` `l`, drag, sideways scroll | move |
| `0` | fit |
| `Esc`, `q` | close |

## Settings

None of its own; it draws with the terminal's image protocol (`[images] protocol`), best with kitty graphics.

## Code and tests

- `vista-tui/src/picture.rs` (`PictureView`: loading, the visible `region` for a zoom and centre, zooming around a point, moving, the frame job), `ui/picture.rs` (drawing), `App::open_picture` / `picture_at` / `on_picture` in `app.rs` (opening on click or `v`, frames in), `Diagram::draw` in `vista-render/src/mermaid.rs` (any region of a diagram at any size). Markdown pictures are drawn with ratatui-image's `SlicedImage`, which shows rows of a picture when the rest is out of view (`draw_markdown` in `ui/viewer.rs`).
- Unit: the region at zoom 1 and zoomed, edges, zooming around the mouse, zoom limits, keys and dragging, cropping and scaling images (`picture`).
- E2e: `e2e_r1_picture_viewer.rs` (an image half in view, cut at the bottom and the top; an image tab opened with `v` and by a click, zoomed, closed), `e2e_r1_mermaid.rs` (a diagram half in view; a diagram opened by a click and with `v`, zoomed with `+`, the wheel and `0`).

See also: [25 · Mermaid diagrams](25-mermaid-diagrams.md), [03 · Markdown and images](03-markdown-and-images.md).
