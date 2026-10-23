//! The control socket (`<runtime dir>/<pid>.sock`): other programs ask a
//! running vista for a project, a review or the Commit window.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use vista_e2e::{Fixture, Repo, Vista};

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// `main` has a.txt and b.txt; `work` changes and renames them, and has an
/// uncommitted change and a new file.
fn repo() -> Repo {
    Fixture::git()
        .file("a.txt", "one\n")
        .file("b.txt", "bee\n")
        .commit("start")
        .run(&["switch", "-q", "-c", "work"])
        .modify("a.txt", "one\ntwo\n")
        .run(&["mv", "b.txt", "c.txt"])
        .commit("agent: a grows, b becomes c")
        .modify("a.txt", "one\ntwo\nthree\n")
        .file("d.txt", "dee\n")
        .build()
}

fn wait_for(path: &Path, present: bool) {
    let start = Instant::now();
    while path.exists() != present {
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "{} still {}",
            path.display(),
            if present { "missing" } else { "there" }
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One request, one response line.
fn ask(sock: &Path, request: &str) -> Value {
    let mut s = UnixStream::connect(sock).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    writeln!(s, "{request}").unwrap();
    let mut line = String::new();
    BufReader::new(&s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line:?}"))
}

fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap()
}

#[test]
fn status_review_projects_and_commit_through_the_socket() {
    let repo = repo();
    let other = Fixture::git().file("zed.txt", "z\n").commit("init").build();
    let run = tempfile::tempdir().unwrap();
    let dir = run.path().join("rt");
    let mut vista = Vista::launch(BIN, &repo)
        .arg("--only")
        .env("VISTA_RUNTIME_DIR", dir.to_str().unwrap())
        .env("HERDR_PANE_ID", "w1:p3")
        .size(150, 40)
        .start();
    vista.expect_text("a.txt");
    let pid = vista.pid().unwrap();
    let sock = dir.join(format!("{pid}.sock"));
    wait_for(&sock, true);
    // Only for us: the folder and the socket.
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600);

    let root = canonical(repo.path());
    let status = ask(&sock, r#"{"cmd":"status"}"#);
    assert_eq!(status["ok"], true);
    assert_eq!(status["protocol"], 1);
    assert_eq!(status["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(status["pid"], pid);
    assert_eq!(status["only"], true);
    assert_eq!(status["herdr_pane"], "w1:p3");
    assert_eq!(status["projects"][0]["root"], root.to_str().unwrap());
    assert_eq!(status["projects"][0]["active"], true);

    // A review, while a popup is open: the popup goes, the review shows.
    vista.press("ctrl+p");
    vista.expect_text("include ignored");
    let base = repo.git(&["rev-parse", "main"]).trim().to_string();
    let req = serde_json::json!({ "cmd": "review", "root": root, "base": base, "label": null });
    let r = ask(&sock, &req.to_string());
    assert_eq!(r, serde_json::json!({ "ok": true, "files": 3, "commits": 1 }));
    vista.expect_text("R: Review");
    vista.expect_text(&format!("Review · 3 files since {}", &base[..7]));
    vista.expect_no_text("include ignored");

    // Another project: opened and shown.
    let other_root = canonical(other.path());
    let req = serde_json::json!({ "cmd": "open-project", "root": other_root });
    assert_eq!(ask(&sock, &req.to_string()), serde_json::json!({ "ok": true }));
    vista.expect_text("zed.txt");
    let status = ask(&sock, r#"{"cmd":"status"}"#);
    assert_eq!(status["projects"][1]["root"], other_root.to_str().unwrap());
    assert_eq!(status["projects"][1]["active"], true);

    // The Commit window of the first one.
    let req = serde_json::json!({ "cmd": "commit", "root": root });
    assert_eq!(ask(&sock, &req.to_string())["ok"], true);
    vista.expect_text("2: Commit");
    vista.expect_text_in("d.txt", 0..40);

    // A review of HEAD is the Commit window.
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let req = serde_json::json!({ "cmd": "review", "root": root, "base": head });
    assert_eq!(
        ask(&sock, &req.to_string()),
        serde_json::json!({ "ok": true, "commit_window": true })
    );

    // A file, at a line.
    let req = serde_json::json!({ "cmd": "open-file", "path": root.join("c.txt"), "line": 1 });
    assert_eq!(ask(&sock, &req.to_string())["ok"], true);
    vista.expect_text("bee");

    // Mistakes are answered, and vista carries on.
    let bad = ask(&sock, "this is not json");
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().starts_with("bad JSON"), "{bad}");
    assert_eq!(ask(&sock, r#"{"cmd":"dance"}"#)["error"], "unknown command `dance`");
    let req = serde_json::json!({ "cmd": "review", "root": root, "base": "nope" });
    assert_eq!(ask(&sock, &req.to_string())["error"], "`nope` is not a commit here");
    assert_eq!(ask(&sock, r#"{"cmd":"commit","root":"relative"}"#)["ok"], false);

    // `vista --remote` does the same from a shell.
    let out = std::process::Command::new(BIN)
        .args(["--remote", &pid.to_string(), r#"{"cmd":"status"}"#])
        .env("VISTA_RUNTIME_DIR", &dir)
        .output()
        .unwrap();
    let status: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(status["ok"], true);
    assert_eq!(status["projects"].as_array().unwrap().len(), 2);

    // Quitting removes the socket.
    vista.press("ctrl+q");
    assert_eq!(vista.expect_exit(), 0);
    wait_for(&sock, false);
}

#[test]
fn sigterm_and_sighup_quit_and_remove_the_socket() {
    for signal in ["-TERM", "-HUP"] {
        let repo = Fixture::git().file("a.txt", "one\n").commit("init").build();
        let run = tempfile::tempdir().unwrap();
        let dir = run.path().join("rt");
        let mut vista = Vista::launch(BIN, &repo)
            .env("VISTA_RUNTIME_DIR", dir.to_str().unwrap())
            .size(100, 30)
            .start();
        vista.expect_text("a.txt");
        let pid = vista.pid().unwrap();
        let sock = dir.join(format!("{pid}.sock"));
        wait_for(&sock, true);
        assert_eq!(ask(&sock, r#"{"cmd":"status"}"#)["ok"], true);
        let killed = std::process::Command::new("kill")
            .args([signal, &pid.to_string()])
            .status()
            .unwrap();
        assert!(killed.success());
        vista.expect_exit();
        wait_for(&sock, false);
    }
}
