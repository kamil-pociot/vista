//! Command-line arguments.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run,
    Help,
    Version,
    /// Report what the terminal supports; `--probe-image` also draws a test image.
    Probe {
        image: bool,
    },
    ListThemes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub path: Option<PathBuf>,
    pub theme: Option<String>,
}

pub const USAGE: &str = "\
vista — a lean, IntelliJ-style terminal file browser

USAGE:
    vista [PATH] [OPTIONS]

OPTIONS:
    --theme <ID>      Use a look (darcula, quiet, paper, or a file in ~/.config/vista/themes)
    --list-themes     List available looks
    --probe           Report terminal capabilities (images, keyboard protocol)
    --probe-image     Like --probe, and draw a test image
    -h, --help        Show this help
    -V, --version     Show the version
";

impl Args {
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Args, String> {
        let mut out = Args {
            command: Command::Run,
            path: None,
            theme: None,
        };
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "-h" | "--help" => out.command = Command::Help,
                "-V" | "--version" => out.command = Command::Version,
                "--probe" => out.command = Command::Probe { image: false },
                "--probe-image" => out.command = Command::Probe { image: true },
                "--list-themes" => out.command = Command::ListThemes,
                "--theme" => out.theme = Some(it.next().ok_or("--theme needs a value")?),
                s if s.starts_with("--theme=") => out.theme = Some(s["--theme=".len()..].to_string()),
                s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option `{s}`")),
                s => {
                    if out.path.is_some() {
                        return Err(format!("unexpected argument `{s}`"));
                    }
                    out.path = Some(PathBuf::from(s));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<Args, String> {
        Args::parse(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses_path_and_theme() {
        let a = parse(&["src", "--theme", "quiet"]).unwrap();
        assert_eq!(a.command, Command::Run);
        assert_eq!(a.path, Some(PathBuf::from("src")));
        assert_eq!(a.theme.as_deref(), Some("quiet"));
        assert_eq!(parse(&["--theme=paper"]).unwrap().theme.as_deref(), Some("paper"));
    }

    #[test]
    fn parses_commands_and_rejects_junk() {
        assert_eq!(
            parse(&["--probe-image"]).unwrap().command,
            Command::Probe { image: true }
        );
        assert_eq!(parse(&["-V"]).unwrap().command, Command::Version);
        assert!(parse(&["--nope"]).is_err());
        assert!(parse(&["a", "b"]).is_err());
        assert!(parse(&["--theme"]).is_err());
    }
}
