//! The main loop: draw, wait for one event, handle it and everything queued
//! behind it, repeat.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event;

use crate::event::{AppEvent, EventTx};
use crate::terminal;
use crate::workspace::Workspace;

const TICK: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(250);

pub fn run(mut ws: Workspace) -> Result<()> {
    let t = std::time::Instant::now();
    let title = |ws: &mut Workspace| ws.title();
    let mut shown_title = title(&mut ws);
    let (mut term, _guard) = terminal::setup(&shown_title)?;
    tracing::info!("terminal ready in {:?}", t.elapsed());
    if ws.active_app().config.images.protocol == vista_core::ImageProtocol::Auto {
        // Must run before the input thread starts reading stdin.
        match ratatui_image::picker::Picker::from_query_stdio() {
            Ok(p) => {
                tracing::info!("image protocol: {:?}, font {:?}", p.protocol_type(), p.font_size());
                ws.set_picker(Some(p));
            }
            Err(e) => tracing::warn!("image protocol query failed ({e}); using halfblocks"),
        }
    } else {
        let picker = ws.active_app().picker.clone();
        ws.set_picker(picker);
    }
    let paused = Arc::new(AtomicBool::new(false));
    spawn_input_thread(ws.input_tx(), paused.clone());
    // Show the first frame before starting watchers and background jobs.
    let t = std::time::Instant::now();
    term.draw(|f| ws.draw(f))?;
    tracing::info!("first frame drawn in {:?}", t.elapsed());
    let active = ws.active;
    ws.activate(active);
    loop {
        term.draw(|f| ws.draw(f))?;
        if ws.should_quit {
            break;
        }
        match ws.rx.recv_timeout(POLL) {
            Ok((id, ev)) => {
                ws.handle(id, ev);
                ws.drain();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        ws.follow_herdr();
        let now = title(&mut ws);
        if now != shown_title {
            terminal::set_title(&now);
            shown_title = now;
        }
        let app = ws.active_app();
        if let Some((editor, path)) = app.take_external() {
            // Hand the terminal to the external editor; stop reading input meanwhile.
            paused.store(true, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(80));
            terminal::suspend();
            let result = run_editor(&editor, &path);
            terminal::resume()?;
            // A fresh terminal has empty buffers, so the next draw repaints
            // everything (Terminal::clear would query the cursor position).
            term = ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(std::io::stdout()))?;
            paused.store(false, Ordering::SeqCst);
            app.after_external(&path, result);
        }
        if app.tick_due(TICK) {
            app.tick();
        }
        if ws.should_quit {
            break;
        }
    }
    ws.on_exit();
    Ok(())
}

/// Runs `$EDITOR file` in the foreground. The editor command may contain
/// arguments (`code --wait`), so it goes through the shell.
fn run_editor(editor: &str, path: &std::path::Path) -> std::io::Result<std::process::ExitStatus> {
    std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("{editor} \"$1\""))
        .arg("vista-editor")
        .arg(path)
        .status()
}

fn spawn_input_thread(tx: EventTx, paused: Arc<AtomicBool>) {
    thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            loop {
                if paused.load(Ordering::SeqCst) {
                    thread::sleep(Duration::from_millis(20));
                    continue;
                }
                match event::poll(Duration::from_millis(50)) {
                    Ok(true) => match event::read() {
                        Ok(ev) => {
                            if tx.send(AppEvent::Term(ev)).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    Ok(false) => {}
                    Err(_) => break,
                }
            }
        })
        .expect("spawn input thread");
}
