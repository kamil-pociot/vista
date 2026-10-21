//! Performance checks against a real repository (opt-in):
//!
//! ```sh
//! VISTA_PERF_REPO=~/Repositories/lms-context cargo test --release --test perf -- --ignored --nocapture
//! ```

use std::time::Instant;

use vista_e2e::Vista;

const BIN: &str = env!("CARGO_BIN_EXE_vista");

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
    assert!(first_frame.as_millis() < 1500, "first frame took {first_frame:?}");
    assert!(status.as_millis() < 3000, "status took {status:?}");
}
