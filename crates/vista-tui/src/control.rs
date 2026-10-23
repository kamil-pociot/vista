//! The control socket: other programs (herdr-vista) ask a running vista to
//! show a project, a review or the Commit window.
//!
//! Each interactive vista listens on `<runtime dir>/<pid>.sock`, so it can be
//! found from its process id. One JSON request per line, one JSON response
//! line, then the connection closes. A listener thread reads the request and
//! does any git work (resolving a review's base); the UI thread applies it and
//! replies, so a reply means it's on screen.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::app::ReviewBase;
use crate::event::{AppEvent, EventTx};

/// Bumped when a command changes incompatibly.
pub const PROTOCOL: u32 = 1;

/// How long a request may take, reading it and applying it each.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Unix socket paths are limited (104 bytes on macOS, 108 on Linux).
const MAX_PATH: usize = 100;

/// A request as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Status,
    OpenProject {
        root: PathBuf,
    },
    Review {
        root: PathBuf,
        base: String,
        label: Option<String>,
    },
    Commit {
        root: PathBuf,
    },
    OpenFile {
        path: PathBuf,
        line: Option<usize>,
    },
}

/// A request made ready for the UI thread (paths resolved, git work done).
pub enum Prepared {
    Status,
    OpenProject(PathBuf),
    Review {
        root: PathBuf,
        base: ReviewBase,
        label: Option<String>,
    },
    Commit(PathBuf),
    OpenFile {
        path: PathBuf,
        line: Option<usize>,
        /// The project to open when none has the file: its git root, or its folder.
        root: PathBuf,
    },
}

/// A prepared request and where its response goes.
pub struct Call {
    pub request: Prepared,
    pub reply: mpsc::Sender<Value>,
}

pub fn ok() -> Value {
    json!({ "ok": true })
}

pub fn error(message: impl std::fmt::Display) -> Value {
    json!({ "ok": false, "error": message.to_string() })
}

/// Parses one request line.
pub fn parse(line: &str) -> Result<Request, String> {
    let v: Value = serde_json::from_str(line).map_err(|e| format!("bad JSON: {e}"))?;
    let cmd = v.get("cmd").and_then(Value::as_str).ok_or("missing `cmd`")?;
    // `null` counts as absent.
    let text = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let path = |key: &str| -> Result<PathBuf, String> {
        let p = PathBuf::from(text(key).ok_or_else(|| format!("missing `{key}`"))?);
        if p.is_absolute() {
            Ok(p)
        } else {
            Err(format!("`{key}` must be an absolute path"))
        }
    };
    Ok(match cmd {
        "status" => Request::Status,
        "open-project" => Request::OpenProject { root: path("root")? },
        "review" => Request::Review {
            root: path("root")?,
            base: text("base").ok_or("missing `base`")?,
            label: text("label"),
        },
        "commit" => Request::Commit { root: path("root")? },
        "open-file" => Request::OpenFile {
            path: path("path")?,
            line: match v.get("line") {
                None | Some(Value::Null) => None,
                Some(n) => Some(n.as_u64().ok_or("`line` must be a number")? as usize),
            },
        },
        other => return Err(format!("unknown command `{other}`")),
    })
}

/// Resolves paths and does the git work, off the UI thread.
pub fn prepare(request: Request) -> Result<Prepared, String> {
    let folder = |root: &Path| -> Result<PathBuf, String> {
        match root.canonicalize() {
            Ok(p) if p.is_dir() => Ok(p),
            _ => Err(format!("no such folder: {}", root.display())),
        }
    };
    Ok(match request {
        Request::Status => Prepared::Status,
        Request::OpenProject { root } => Prepared::OpenProject(folder(&root)?),
        Request::Commit { root } => Prepared::Commit(folder(&root)?),
        Request::Review { root, base, label } => {
            let root = folder(&root)?;
            let git =
                vista_git::Git::discover(&root).map_err(|_| format!("not a git repository: {}", root.display()))?;
            Prepared::Review {
                base: crate::app::review_base(&git, &base)?,
                root,
                label,
            }
        }
        Request::OpenFile { path, line } => {
            let path = match path.canonicalize() {
                Ok(p) if p.is_file() => p,
                _ => return Err(format!("no such file: {}", path.display())),
            };
            let dir = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/"));
            let root = vista_git::Git::discover(&dir).map(|g| g.toplevel).unwrap_or(dir);
            Prepared::OpenFile { path, line, root }
        }
    })
}

/// Where sockets go: `$VISTA_RUNTIME_DIR`, else `$XDG_RUNTIME_DIR/vista`,
/// else `$TMPDIR/vista-<uid>`, else `/tmp/vista-<uid>`.
pub fn runtime_dir(env: impl Fn(&str) -> Option<String>, uid: u32) -> PathBuf {
    let set = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = set("VISTA_RUNTIME_DIR") {
        return d;
    }
    if let Some(d) = set("XDG_RUNTIME_DIR") {
        return d.join("vista");
    }
    set("TMPDIR")
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(format!("vista-{uid}"))
}

/// `<dir>/<pid>.sock`, if short enough to bind.
pub fn socket_path(dir: &Path, pid: u32) -> Result<PathBuf, String> {
    let path = dir.join(format!("{pid}.sock"));
    if path.as_os_str().len() > MAX_PATH {
        return Err(format!(
            "socket path too long ({} bytes, max {MAX_PATH}): {}",
            path.as_os_str().len(),
            path.display()
        ));
    }
    Ok(path)
}

/// This process's socket path, from the environment.
pub fn own_socket_path() -> Result<PathBuf, String> {
    let dir = runtime_dir(|k| std::env::var(k).ok(), current_uid());
    socket_path(&dir, std::process::id())
}

fn current_uid() -> u32 {
    // SAFETY: getuid has no preconditions and can't fail.
    unsafe { libc::getuid() }
}

/// The listening socket; the file goes when this is dropped.
pub struct Server {
    path: PathBuf,
}

impl Server {
    /// Starts listening; on failure logs why and returns `None` (vista runs
    /// without a socket).
    pub fn start(tx: EventTx) -> Option<Server> {
        match Server::bind(tx) {
            Ok(s) => {
                tracing::info!("control socket: {}", s.path.display());
                Some(s)
            }
            Err(e) => {
                tracing::warn!("no control socket: {e}");
                None
            }
        }
    }

    fn bind(tx: EventTx) -> Result<Server, String> {
        Server::bind_at(own_socket_path()?, tx)
    }

    fn bind_at(path: PathBuf, tx: EventTx) -> Result<Server, String> {
        let dir = path.parent().ok_or("no runtime dir")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        // Only ours, and only we can get in.
        let meta = std::fs::metadata(dir).map_err(|e| e.to_string())?;
        if meta.uid() != current_uid() {
            return Err(format!("{} belongs to another user", dir.display()));
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
        // A leftover from an earlier process with this pid.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let server = Server { path };
        std::fs::set_permissions(&server.path, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        std::thread::Builder::new()
            .name("control".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let tx = tx.clone();
                    // One at a time is enough, but a stuck client mustn't block others.
                    let _ = std::thread::Builder::new()
                        .name("control-client".into())
                        .spawn(move || serve(stream, &tx));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(server)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Answers one connection.
fn serve(stream: UnixStream, tx: &EventTx) {
    let _ = stream.set_read_timeout(Some(TIMEOUT));
    let _ = stream.set_write_timeout(Some(TIMEOUT));
    let mut line = String::new();
    let read = BufReader::new(&stream).take(1 << 20).read_line(&mut line);
    let response = match read {
        Ok(0) => error("empty request"),
        Ok(_) => respond(&line, tx),
        Err(e) => error(format!("reading the request: {e}")),
    };
    let mut out = &stream;
    let _ = writeln!(out, "{response}");
    let _ = out.flush();
}

fn respond(line: &str, tx: &EventTx) -> Value {
    let request = match parse(line.trim()).and_then(prepare) {
        Ok(r) => r,
        Err(e) => return error(e),
    };
    let (reply, answer) = mpsc::channel();
    if tx.send(AppEvent::Control(Box::new(Call { request, reply }))).is_err() {
        return error("vista is quitting");
    }
    answer
        .recv_timeout(TIMEOUT)
        .unwrap_or_else(|_| error("vista didn't answer in time"))
}

/// `vista --remote <pid> '<json>'`: sends one request to that vista's
/// socket and returns the response line.
pub fn remote(pid: u32, request: &str) -> Result<String, String> {
    let dir = runtime_dir(|k| std::env::var(k).ok(), current_uid());
    let path = socket_path(&dir, pid)?;
    let mut stream = UnixStream::connect(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let _ = stream.set_read_timeout(Some(TIMEOUT * 2));
    writeln!(stream, "{}", request.trim()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    Ok(line.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_command() {
        assert_eq!(parse(r#"{"cmd":"status"}"#), Ok(Request::Status));
        assert_eq!(
            parse(r#"{"cmd":"open-project","root":"/r"}"#),
            Ok(Request::OpenProject { root: "/r".into() })
        );
        assert_eq!(
            parse(r#"{"cmd":"review","root":"/r","base":"abc","label":"fix auth"}"#),
            Ok(Request::Review {
                root: "/r".into(),
                base: "abc".into(),
                label: Some("fix auth".into()),
            })
        );
        // A null label is no label.
        assert_eq!(
            parse(r#"{"cmd":"review","root":"/r","base":"abc","label":null}"#),
            Ok(Request::Review {
                root: "/r".into(),
                base: "abc".into(),
                label: None,
            })
        );
        assert_eq!(
            parse(r#"{"cmd":"commit","root":"/r"}"#),
            Ok(Request::Commit { root: "/r".into() })
        );
        assert_eq!(
            parse(r#"{"cmd":"open-file","path":"/r/a.rs","line":42}"#),
            Ok(Request::OpenFile {
                path: "/r/a.rs".into(),
                line: Some(42),
            })
        );
        assert_eq!(
            parse(r#"{"cmd":"open-file","path":"/r/a.rs"}"#),
            Ok(Request::OpenFile {
                path: "/r/a.rs".into(),
                line: None,
            })
        );
    }

    #[test]
    fn rejects_what_it_cant_do() {
        assert!(parse("nope").unwrap_err().starts_with("bad JSON"));
        assert_eq!(parse(r#"{"cmd":"dance"}"#), Err("unknown command `dance`".into()));
        assert_eq!(parse(r#"{"root":"/r"}"#), Err("missing `cmd`".into()));
        assert_eq!(parse(r#"{"cmd":"review","root":"/r"}"#), Err("missing `base`".into()));
        assert_eq!(parse(r#"{"cmd":"commit"}"#), Err("missing `root`".into()));
        assert_eq!(
            parse(r#"{"cmd":"commit","root":"rel/path"}"#),
            Err("`root` must be an absolute path".into())
        );
        assert!(parse(r#"{"cmd":"open-file","path":"/a","line":"x"}"#).is_err());
        assert!(
            prepare(Request::OpenProject {
                root: "/no/such/dir".into()
            })
            .is_err()
        );
    }

    #[test]
    fn runtime_dir_in_order_of_preference() {
        fn env<'a>(pairs: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
            move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
        }
        let all = [
            ("VISTA_RUNTIME_DIR", "/v"),
            ("XDG_RUNTIME_DIR", "/x"),
            ("TMPDIR", "/t/"),
        ];
        assert_eq!(runtime_dir(env(&all), 501), PathBuf::from("/v"));
        assert_eq!(runtime_dir(env(&all[1..]), 501), PathBuf::from("/x/vista"));
        assert_eq!(runtime_dir(env(&all[2..]), 501), PathBuf::from("/t/vista-501"));
        assert_eq!(runtime_dir(env(&[]), 501), PathBuf::from("/tmp/vista-501"));
        // Empty is unset.
        assert_eq!(
            runtime_dir(env(&[("VISTA_RUNTIME_DIR", "")]), 7),
            PathBuf::from("/tmp/vista-7")
        );
    }

    #[test]
    fn binds_over_a_leftover_and_answers() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rt/42.sock");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "left by a vista that crashed").unwrap();
        let (tx, rx) = crate::event::channel(0);
        let server = Server::bind_at(path.clone(), tx).unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!((mode(path.parent().unwrap()), mode(&path)), (0o700, 0o600));
        // The UI side: answer whatever comes.
        std::thread::spawn(move || {
            while let Ok((_, ev)) = rx.recv() {
                if let AppEvent::Control(call) = ev {
                    let _ = call.reply.send(json!({ "ok": true, "seen": true }));
                }
            }
        });
        let ask = |req: &str| {
            let mut s = UnixStream::connect(&path).unwrap();
            writeln!(s, "{req}").unwrap();
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).unwrap();
            serde_json::from_str::<Value>(&line).unwrap()
        };
        assert_eq!(ask(r#"{"cmd":"status"}"#), json!({ "ok": true, "seen": true }));
        // Refused before reaching the UI.
        assert_eq!(ask(r#"{"cmd":"nope"}"#)["ok"], false);
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn socket_paths_must_fit() {
        assert_eq!(
            socket_path(Path::new("/tmp/vista-501"), 123),
            Ok(PathBuf::from("/tmp/vista-501/123.sock"))
        );
        let long = PathBuf::from(format!("/{}", "d".repeat(95)));
        assert!(socket_path(&long, 123).unwrap_err().contains("too long"));
    }
}
