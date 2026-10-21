//! The main loop: draw, wait for one event, handle it and everything queued
//! behind it, repeat.

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
    spawn_input_thread(app.tx.clone());
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

fn spawn_input_thread(tx: Sender<AppEvent>) {
    thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            while let Ok(ev) = event::read() {
                if tx.send(AppEvent::Term(ev)).is_err() {
                    break;
                }
            }
        })
        .expect("spawn input thread");
}
