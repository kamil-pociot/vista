//! Drawing Preferences: the Settings page and the Keymap page.

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::keymap::Action;
use crate::keymap_page::Mode;
use crate::prefs::{Page, Pref, SECTIONS};
use crate::style::ThemeStyle;
use crate::ui::click::{self, Click};

/// One line of the page: a heading, a setting (its index among all), or a gap.
enum Row {
    Heading(&'static str),
    Setting(usize, Pref),
    Gap,
}

fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let mut i = 0;
    for (n, (title, prefs)) in SECTIONS.iter().enumerate() {
        if n > 0 {
            rows.push(Row::Gap);
        }
        rows.push(Row::Heading(title));
        for p in *prefs {
            rows.push(Row::Setting(i, *p));
            i += 1;
        }
    }
    rows
}

pub fn draw(f: &mut Frame, area: Rect, app: &App) {
    let Some(page) = app.prefs.as_ref() else { return };
    let t = &app.theme;
    // One size for both pages, so switching doesn't move the frame.
    let w = area.width.saturating_sub(4).min(78);
    let tallest = rows().len().max(Action::ALL.len() + 1) as u16;
    let h = (tallest + 7).min(area.height.saturating_sub(2));
    let rect = crate::ui::help::centered(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(t.fg("ui.popup.border"))
        .style(t.fg_bg("ui.text", "ui.popup"))
        .title(Span::styled(
            " Preferences ",
            t.fg("ui.text.strong").add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    if inner.height < 6 || inner.width < 20 {
        return;
    }
    page_switcher(f, Rect { height: 1, ..inner }, app, page.page);
    // The pages, then a rule, the selected row's explanation (2 rows) and the keys.
    let body = Rect {
        y: inner.y + 1,
        height: inner.height - 1,
        ..inner
    };
    let list = Rect {
        height: body.height.saturating_sub(4),
        ..body
    };
    let rule = Rect {
        y: list.y + list.height,
        height: 1,
        ..body
    };
    f.render_widget(
        Paragraph::new(Span::styled("─".repeat(body.width as usize), t.fg("ui.popup.border"))),
        rule,
    );
    let about = Rect {
        y: rule.y + 1,
        height: 2,
        ..body
    };
    let keys = Rect {
        y: rule.y + 3,
        height: 1,
        ..body
    };
    match page.page {
        Page::Settings => settings(f, list, about, keys, app),
        Page::Keymap => keymap(f, list, about, keys, app),
    }
}

/// `Settings  Keymap` at the top; the one shown stands out, both click.
fn page_switcher(f: &mut Frame, area: Rect, app: &App, shown: Page) {
    let t = &app.theme;
    let mut items: Vec<click::Item> = vec![(Span::raw(" "), None)];
    for (p, name) in [(Page::Settings, "Settings"), (Page::Keymap, "Keymap")] {
        let style = if p == shown {
            t.fg_bg("ui.text.strong", "ui.selection").add_modifier(Modifier::BOLD)
        } else {
            t.fg("ui.dim")
        };
        items.push((Span::styled(format!(" {name} "), style), Some(Click::PrefsPage(p))));
        items.push((Span::raw(" "), None));
    }
    items.push((Span::styled("  Tab switches", t.fg("ui.dim")), None));
    click::line(f, area, items, Alignment::Left, Style::default());
}

fn settings(f: &mut Frame, list: Rect, about: Rect, keys: Rect, app: &App) {
    let Some(page) = app.prefs.as_ref() else { return };
    let t = &app.theme;
    let rows = rows();
    let inner = list;
    let list_h = list.height;
    let selected = page.selected;
    let at = rows
        .iter()
        .position(|r| matches!(r, Row::Setting(i, _) if *i == selected))
        .unwrap_or(0);
    // Keep the selected row (and its heading, near the top) in view.
    let scroll = (at + 1).saturating_sub(list_h as usize);
    let width = inner.width as usize;
    for (k, row) in rows.iter().skip(scroll).take(list_h as usize).enumerate() {
        let r = Rect {
            y: inner.y + k as u16,
            height: 1,
            ..inner
        };
        match row {
            Row::Gap => {}
            Row::Heading(title) => {
                let style = t.fg("ui.header").add_modifier(Modifier::BOLD);
                f.render_widget(Paragraph::new(Span::styled(format!(" {title}"), style)), r);
            }
            Row::Setting(i, p) => {
                let value = app.pref_value(*p);
                let value = match p {
                    Pref::Look => format!("◂ {value} ▸"),
                    _ => value,
                };
                let value_style = match value.as_str() {
                    "On" => t.fg("ui.accent").add_modifier(Modifier::BOLD),
                    "Off" => t.fg("ui.dim"),
                    _ => t.fg("ui.text.strong"),
                };
                let label = format!("   {}", p.label());
                let pad = width.saturating_sub(label.width() + value.width() + 2);
                let line = Line::from(vec![
                    Span::styled(label, t.fg("ui.text")),
                    Span::raw(" ".repeat(pad)),
                    Span::styled(value, value_style),
                    Span::raw("  "),
                ]);
                let style = if *i == selected {
                    t.bg("ui.selection")
                } else {
                    Style::default()
                };
                f.render_widget(Paragraph::new(line).style(style), r);
                click::add(r, Click::Pref(*i));
            }
        }
    }
    // Below the list: what the selected setting does, then the keys.
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {}", page.selected_pref().about()),
            t.fg("ui.dim"),
        ))
        .wrap(Wrap { trim: false }),
        about,
    );
    let mut items: Vec<click::Item> = vec![(Span::styled(" ↑↓ select · ←→ / Space change · ", t.fg("ui.dim")), None)];
    items.extend(click::hints(
        &[
            ("Tab Keymap", click::key(KeyCode::Tab, None)),
            ("Esc close", click::key(KeyCode::Esc, None)),
        ],
        " · ",
        t.fg("ui.dim"),
    ));
    click::line(f, keys, items, Alignment::Left, Style::default());
}

fn keymap(f: &mut Frame, list: Rect, about: Rect, keys: Rect, app: &App) {
    let Some(prefs) = app.prefs.as_ref() else { return };
    let page = &prefs.keymap;
    let (t, km, base) = (&app.theme, &app.keymap, &app.keymap_base);
    let actions = page.rows(km);
    let selected = page.selected.min(actions.len().saturating_sub(1));
    let mut list = list;
    if page.filtering || !page.filter.is_empty() {
        let mut spans = vec![
            Span::styled(" Filter: ", t.fg("ui.text.strong")),
            Span::styled(page.filter.clone(), t.fg("ui.text")),
        ];
        if page.filtering {
            spans.push(Span::styled("▏", t.fg("ui.text")));
        }
        if actions.is_empty() {
            spans.push(Span::styled("   no actions", t.fg("ui.error")));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), Rect { height: 1, ..list });
        list.y += 1;
        list.height = list.height.saturating_sub(1);
    }
    let scroll = (selected + 1).saturating_sub(list.height as usize);
    let width = list.width as usize;
    for (k, action) in actions.iter().enumerate().skip(scroll).take(list.height as usize) {
        let r = Rect {
            y: list.y + (k - scroll) as u16,
            height: 1,
            ..list
        };
        let is_selected = k == selected;
        let chords = km.chords_for(*action);
        // The keys, right-aligned: (text, style, chip index).
        let mut chips: Vec<(String, Style, Option<usize>)> = Vec::new();
        if is_selected && page.mode == Mode::Capture {
            chips.push((" press a key… ".into(), t.fg_bg("ui.match.text", "ui.accent"), None));
        } else if chords.is_empty() {
            chips.push(("—".into(), t.fg("ui.dim"), None));
        }
        if !(is_selected && page.mode == Mode::Capture) {
            for (ci, c) in chords.iter().enumerate() {
                let style = if is_selected && ci == page.chip.min(chords.len() - 1) {
                    t.fg_bg("ui.match.text", "ui.accent")
                } else {
                    t.fg_bg("ui.text.strong", "ui.panel")
                };
                chips.push((format!(" {c} "), style, Some(ci)));
            }
        }
        let chips_w: usize = chips.iter().map(|(s, ..)| s.width() + 1).sum::<usize>() + 1;
        // Changed from the default: the name stands out.
        let changed = chords != base.chords_for(*action);
        let mut label = format!("   {}", action.description());
        let room = width.saturating_sub(chips_w + 1);
        if label.width() > room {
            label = label.chars().take(room.saturating_sub(1)).collect::<String>() + "…";
        }
        let label_style = if changed { t.fg("ui.accent") } else { t.fg("ui.text") };
        let pad = width.saturating_sub(label.width() + chips_w);
        let mut spans = vec![Span::styled(label.clone(), label_style), Span::raw(" ".repeat(pad))];
        let mut x = r.x + (label.width() + pad) as u16;
        let mut chip_rects = Vec::new();
        for (text, style, ci) in chips {
            let w = text.width() as u16;
            if let Some(ci) = ci {
                chip_rects.push((Rect { x, width: w, ..r }, ci));
            }
            spans.push(Span::styled(text, style));
            spans.push(Span::raw(" "));
            x += w + 1;
        }
        let row_style = if is_selected {
            t.bg("ui.selection")
        } else {
            Style::default()
        };
        f.render_widget(Paragraph::new(Line::from(spans)).style(row_style), r);
        click::add(r, Click::KeymapRow(k, None));
        for (rect, ci) in chip_rects {
            click::add(rect, Click::KeymapRow(k, Some(ci)));
        }
    }
    // Below the list: the selected action, or what the page waits for.
    let action = actions.get(selected).copied();
    let keys_of = |a: Action, km: &crate::keymap::Keymap| {
        let c: Vec<String> = km.chords_for(a).iter().map(ToString::to_string).collect();
        if c.is_empty() {
            "no key".to_string()
        } else {
            c.join(", ")
        }
    };
    let (text, token) = match (page.mode, action) {
        (_, None) => (String::new(), "ui.dim"),
        (Mode::Capture, Some(a)) => (
            format!("Press the key for {}. Esc cancels.", a.description()),
            "ui.text.strong",
        ),
        (Mode::Conflict(c, other), Some(a)) => (
            format!(
                "{c} runs {}. Enter: use it for {} instead · Esc: keep it",
                other.description(),
                a.description()
            ),
            "ui.error",
        ),
        (Mode::ConfirmResetAll, _) => (
            "Reset every key to its default (config.toml's [keys] stay)? Enter: reset · Esc: keep".into(),
            "ui.error",
        ),
        (Mode::Browse, Some(a)) => match &page.note {
            Some(n) => (n.clone(), "ui.text.strong"),
            None if km.chords_for(a) != base.chords_for(a) => (
                format!("{} · changed; r resets it to {}", a.name(), keys_of(a, base)),
                "ui.dim",
            ),
            None => (format!("{} · default", a.name()), "ui.dim"),
        },
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {text}"), t.fg(token))).wrap(Wrap { trim: false }),
        about,
    );
    let mut items: Vec<click::Item> = vec![(Span::raw(" "), None)];
    items.extend(click::hints(
        &[
            ("Enter add", click::key(KeyCode::Enter, None)),
            ("Del remove", click::key(KeyCode::Delete, None)),
            ("r reset", click::key(KeyCode::Char('r'), None)),
            ("R reset all", click::key(KeyCode::Char('R'), None)),
            ("/ filter", click::key(KeyCode::Char('/'), None)),
            ("Esc close", click::key(KeyCode::Esc, None)),
        ],
        " · ",
        t.fg("ui.dim"),
    ));
    click::line(f, keys, items, Alignment::Left, Style::default());
}
