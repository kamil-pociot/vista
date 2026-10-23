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
    /// `--remote <pid> <json>`: send a control-socket request to that vista.
    Remote {
        pid: u32,
        request: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    pub path: Option<PathBuf>,
    pub theme: Option<String>,
    /// `--only`: just this project (no restore), and don't remember which
    /// projects are open (several vistas, one per repository, as herdr runs them).
    pub only: bool,
    /// `--review <rev>`: open the Review panel on what changed since `rev`.
    pub review: Option<String>,
    /// `--review-label <text>`: what's being reviewed, shown in the panel.
    pub review_label: Option<String>,
}

pub const USAGE: &str = "\
vista — a lean, IntelliJ-style terminal file browser

USAGE:
    vista [PATH] [OPTIONS]

OPTIONS:
    --only            Open just this project: don't reopen last time's, or remember which are open
    --review <REV>    Review what changed since REV (committed, uncommitted, untracked)
    --review-label <TEXT>
                      What's being reviewed, shown in the Review panel
    --theme <ID>      Use a look (darcula, quiet, paper, or a file in ~/.config/vista/themes)
    --list-themes     List available looks
    --remote <PID> <JSON>
                      Send a request to the vista with that pid (its control socket) and print the answer
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
            only: false,
            review: None,
            review_label: None,
        };
        let mut it = args.into_iter();
        while let Some(a) = it.next() {
            match a.as_str() {
                "-h" | "--help" => out.command = Command::Help,
                "-V" | "--version" => out.command = Command::Version,
                "--probe" => out.command = Command::Probe { image: false },
                "--probe-image" => out.command = Command::Probe { image: true },
                "--list-themes" => out.command = Command::ListThemes,
                "--only" => out.only = true,
                "--remote" => {
                    let pid = it.next().ok_or("--remote needs a pid and a request")?;
                    let pid = pid.parse().map_err(|_| format!("--remote: `{pid}` is not a pid"))?;
                    let request = it.next().ok_or("--remote needs a request (JSON)")?;
                    out.command = Command::Remote { pid, request };
                }
                "--review" => out.review = Some(it.next().ok_or("--review needs a revision")?),
                "--review-label" => out.review_label = Some(it.next().ok_or("--review-label needs a value")?),
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
    fn parses_only_with_and_without_a_path() {
        let a = parse(&["--only", "src"]).unwrap();
        assert!(a.only);
        assert_eq!(a.path, Some(PathBuf::from("src")));
        let a = parse(&["--only"]).unwrap();
        assert!(a.only && a.path.is_none());
        assert!(!parse(&["src"]).unwrap().only);
        assert!(parse(&["--theme", "nord", "--only", "x"]).unwrap().only);
    }

    #[test]
    fn parses_review_and_its_label() {
        let a = parse(&["--only", "--review", "abc123", "--review-label", "fix auth", "repo"]).unwrap();
        assert_eq!(a.review.as_deref(), Some("abc123"));
        assert_eq!(a.review_label.as_deref(), Some("fix auth"));
        assert_eq!(a.path, Some(PathBuf::from("repo")));
        assert!(parse(&["--review"]).is_err());
        assert!(parse(&["--review-label"]).is_err());
    }

    #[test]
    fn parses_remote() {
        assert_eq!(
            parse(&["--remote", "123", r#"{"cmd":"status"}"#]).unwrap().command,
            Command::Remote {
                pid: 123,
                request: r#"{"cmd":"status"}"#.into()
            }
        );
        assert!(parse(&["--remote", "x", "{}"]).is_err());
        assert!(parse(&["--remote", "123"]).is_err());
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
