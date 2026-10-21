use std::fs::OpenOptions;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Mutex;

use anyhow::{Context, Result};
use vista_core::{Config, Paths};
use vista_theme::ThemeLoader;
use vista_tui::args::{Args, Command, USAGE};
use vista_tui::{App, AppOptions, probe, run};

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
        Command::Run => {}
    }
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
    let mut app = App::new(AppOptions {
        root,
        config,
        paths,
        theme: args.theme,
    })
    .context("starting vista")?;
    if let Some(f) = file {
        app.open_file(&f);
    }
    run::run(app)
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
