# 03 · Markdown and images

## Rendered Markdown

Files ending in `.md`, `.markdown`, `.mdown`, `.mkd` or `.mdx` open rendered (pulldown-cmark):

- Headings, emphasis, lists, task lists, block quotes, links, inline images, fenced code with syntax highlighting.
- **Mermaid diagrams** (` ```mermaid ` blocks) as pictures, or drawn with box-drawing text where the terminal has no kitty graphics ([25](25-mermaid-diagrams.md)).
- **Tables show every value.** By default cells wrap to fit the width, with a rule between rows. `w` switches to full-width rows that scroll sideways. Nothing is cut to `…`.
- **Line numbers** (`Shift-L`) show the source line each block comes from, so you can jump to it in the editor.
- `m` switches between rendered and raw source (raw is highlighted as Markdown).
- Markdown opens wrapped; that default is remembered separately from code.
- Text selection and copy work on the rendered view ([02](02-viewer.md#text-selection)).
- Markdown diffs open wrapped as well.

## Images

- png, jpg, gif, webp, bmp, ico and tiff are shown fitted to the pane (`ratatui-image`).
- In terminals with the kitty graphics protocol (Ghostty, kitty, WezTerm) images are drawn as real pixels, with unicode placeholders so they survive scrolling and redraws. Other terminals get coloured half-blocks.
- SVG is shown as source.
- In diffs, changed images are shown side by side.

## Settings

```toml
[images]
protocol = "auto"   # auto | kitty | halfblocks | off
```

`vista --probe` reports whether the terminal supports kitty graphics; `vista --probe-image` draws a test gradient.

## Code and tests

- `vista-render/src/markdown.rs` (`render_with`, `Tables::{Wrap, Wide}`, source-line mapping), `vista-tui/src/viewer.rs`, `ui/viewer.rs`.
- Unit tests: table wrapping, rules, alignment, wide mode. Snapshots: rendered Markdown.
- E2e: `e2e_m2_viewer.rs` (Markdown, kitty sequence emitted, half-blocks when forced), `e2e_r1_feedback.rs` (tables in both modes), `e2e_r1_settings.rs` (source line numbers).

See also: [25 · Mermaid diagrams](25-mermaid-diagrams.md).
