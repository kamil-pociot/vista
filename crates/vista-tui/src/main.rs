use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

use anyhow::{Context, Result};
use vista_core::{Config, Paths};
use vista_theme::ThemeLoader;
use vista_tui::args::{Args, Command, USAGE};
use vista_tui::{Workspace, probe, run};

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("vista: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<()> {
    let args = Args::parse(std::env::args().skip(1)).map_err(|e| anyhow::anyhow!("{e}\n\n{USAGE}"))?;
    let paths = Paths::from_env();
    match args.command {
        Command::Help => {
            print!("{USAGE}");
            return Ok(());
        }
        Command::Version => {
            println!("vista {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Command::Probe { image } => return probe::run(image),
        Command::ListThemes => {
            for id in ThemeLoader::new(Some(paths.themes_dir())).available() {
                println!("{id}");
            }
            return Ok(());
        }
        Command::Remote { pid, request } => {
            return match vista_tui::control::remote(pid, &request) {
                Ok(response) => {
                    println!("{response}");
                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(e)),
            };
        }
        Command::Run => {}
    }
    let started = std::time::Instant::now();
    init_logging(&paths);
    let config = Config::load(&paths)?;
    let path = args.path.unwrap_or_else(|| PathBuf::from("."));
    // `vista some/file.rs` opens the file with its folder as the project.
    let (root, file) = if path.is_file() {
        let file = path.canonicalize()?;
        let root = file.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        (root, Some(file))
    } else {
        (path, None)
    };
    // The projects open last time come back, plus this one (shown); with
    // `--only`, just this one.
    let mut ws = Workspace::new(config, paths, args.theme);
    if args.only {
        ws.persist_open = false;
    } else {
        ws.restore();
    }
    let i = ws.open(&root).context("starting vista")?;
    ws.active = i;
    if let Some(f) = file {
        ws.active_app().open_file(&f);
    }
    if let Some(rev) = &args.review {
        // Worked out before the UI starts; a bad revision is a message, not a refusal.
        let app = ws.active_app();
        match app.git.as_ref().map(|git| vista_tui::app::review_base(git, rev)) {
            Some(Ok(base)) => {
                app.apply_review(base, args.review_label.clone());
            }
            Some(Err(e)) => app.notify(format!("review: {e}"), vista_tui::app::MsgKind::Error),
            None => app.notify("review: not a git repository", vista_tui::app::MsgKind::Error),
        }
    }
    ws.start_control();
    tracing::info!("app ready in {:?}", started.elapsed());
    run::run(ws)
}

fn init_logging(paths: &Paths) {
    if let Some(dir) = paths.log_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&paths.log_file) else {
        return;
    };
    let filter = tracing_subscriber::EnvFilter::try_from_env("VISTA_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(Mutex::new(file))
        .try_init();
}
