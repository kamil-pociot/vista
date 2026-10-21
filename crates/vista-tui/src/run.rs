//! The main loop: draw, wait for one event, handle it and everything queued
//! behind it, repeat.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event;

use crate::app::App;
use crate::event::AppEvent;
use crate::{terminal, ui};

const TICK: Duration = Duration::from_millis(500);
const POLL: Duration = Duration::from_millis(250);

pub fn run(mut app: App) -> Result<()> {
    let (mut term, _guard) = terminal::setup(&format!("vista — {}", app.project_name))?;
    if app.config.images.protocol == vista_core::ImageProtocol::Auto {
        // Must run before the input thread starts reading stdin.
        match ratatui_image::picker::Picker::from_query_stdio() {
            Ok(p) => {
                tracing::info!("image protocol: {:?}, font {:?}", p.protocol_type(), p.font_size());
                app.picker = Some(p);
            }
            Err(e) => tracing::warn!("image protocol query failed ({e}); using halfblocks"),
        }
    }
    let paused = Arc::new(AtomicBool::new(false));
    spawn_input_thread(app.tx.clone(), paused.clone());
    app.on_start();
    loop {
        term.draw(|f| ui::draw(f, &mut app))?;
        if app.should_quit {
            break;
        }
        match app.rx.recv_timeout(POLL) {
            Ok(ev) => {
                app.handle_event(ev);
                app.drain_events();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
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
        if app.should_quit {
            break;
        }
    }
    app.on_exit();
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

fn spawn_input_thread(tx: Sender<AppEvent>, paused: Arc<AtomicBool>) {
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
