//! Performance checks against a real repository (opt-in):
//!
//! ```sh
//! VISTA_PERF_REPO=~/Repositories/lms-context cargo test --release --test perf -- --ignored --nocapture
//! ```

use std::time::Instant;

use vista_e2e::Vista;

const BIN: &str = env!("CARGO_BIN_EXE_vista");

/// Resident memory of a process in KiB (macOS/Linux `ps`).
fn rss_kib(pid: u32) -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

#[test]
#[ignore = "perf baseline"]
fn startup_baseline_on_an_empty_folder() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "x").unwrap();
    for _ in 0..3 {
        let start = Instant::now();
        let mut vista = Vista::launch_dir(BIN, dir.path()).start();
        vista.expect_text("1: Project");
        println!("empty folder first frame: {:?}", start.elapsed());
    }
}

#[test]
#[ignore = "needs VISTA_PERF_REPO"]
fn tree_and_status_are_fast_on_a_real_repo() {
    let Some(repo) = std::env::var_os("VISTA_PERF_REPO") else {
        return;
    };
    let dir = std::path::PathBuf::from(repo);
    let start = Instant::now();
    let mut vista = Vista::launch_dir(BIN, &dir).size(160, 45).start();
    vista.expect_text("1: Project");
    let first_frame = start.elapsed();
    // Status is loaded once the bottom bar shows counts or the branch shows up.
    vista.expect_text("⎇");
    let status = start.elapsed();
    println!("first frame: {first_frame:?}, branch+status: {status:?}");
    for l in vista.log().lines().filter(|l| l.contains(" in ")) {
        println!("  log: {}", l.split(": ").last().unwrap_or(l));
    }
    vista.settle(std::time::Duration::from_millis(500));
    if let Some(kib) = vista.pid().and_then(rss_kib) {
        println!("memory after start: {:.1} MiB", kib as f64 / 1024.0);
    }
    assert!(first_frame.as_millis() < 1500, "first frame took {first_frame:?}");
    assert!(status.as_millis() < 3000, "status took {status:?}");

    // Find in Files: keystroke → first visible hit, and → finished search.
    let query = std::env::var("VISTA_PERF_QUERY").unwrap_or_else(|_| "deploy".into());
    for round in ["cold", "warm"] {
        vista.press("ctrl+f");
        vista.expect_text("File mask:");
        vista.press("ctrl+u");
        vista.settle(std::time::Duration::from_millis(150));
        let start = Instant::now();
        vista.type_text(&query);
        // A selected hit shows its file and occurrence count above the preview.
        vista.expect_text("occurrence");
        let first = start.elapsed();
        vista.wait_until("search finished", |s| {
            let c = s.contents();
            c.contains("matches in") && !c.contains(" 0 matches in")
        });
        let done = start.elapsed();
        println!("find in files ({query}, {round}): first hit on screen {first:?}, finished {done:?}");
        assert!(first.as_millis() < 300, "first results took {first:?}");
        vista.press("esc");
    }
}
