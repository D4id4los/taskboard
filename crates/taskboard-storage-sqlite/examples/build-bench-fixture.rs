// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! Builds a file-backed bench-fixture database for the process smoke
//! harnesses (`scripts/process_smoke.py`, plan D2): the exact
//! `hot_path` bench fixture builder at N tasks, written through the
//! production `open` recipe and verified to load back N tasks — a loud
//! panic otherwise (never silently measure an empty or short database).
//!
//! Usage:
//!
//! ```text
//! build-bench-fixture --out <PATH> [--tasks <N>]
//! ```
//!
//! Prints the absolute database path on success.

// dead_code: the include carries the benches' full builder surface
// (N_SIZES et al.); the example consumes `build_fixture` only.
#[allow(dead_code)]
#[path = "../benches/support/mod.rs"]
mod support;

use std::path::PathBuf;
use std::process::ExitCode;

use taskboard_domain::ConnectorId;
use taskboard_domain::persistence::TaskRepository;
use taskboard_storage_sqlite::open;

struct Args {
    tasks: usize,
    out: PathBuf,
}

/// Hand-parsed flags (no clap in this crate); usage error = exit 2.
fn parse_args() -> Option<Args> {
    let mut tasks = 10_000;
    let mut out = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tasks" => {
                tasks = args.next()?.parse().ok()?;
            }
            "--out" => {
                out = Some(PathBuf::from(args.next()?));
            }
            _ => return None,
        }
    }
    Some(Args { tasks, out: out? })
}

fn main() -> ExitCode {
    let Some(args) = parse_args() else {
        eprintln!("usage: build-bench-fixture --out <PATH> [--tasks <N>] (default N = 10000)");
        return ExitCode::from(2);
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let fixture = support::build_fixture(args.tasks);
    let expected = fixture.state.tasks.len();
    let repo = rt
        .block_on(async { open(args.out.as_path(), ConnectorId::from("nextcloud")).await })
        .expect("open file-backed repository");
    rt.block_on(repo.apply(fixture.actions))
        .expect("population apply");
    let loaded = rt.block_on(repo.load()).expect("population load");
    assert_eq!(
        loaded.tasks.len(),
        expected,
        "the database must hold exactly N tasks"
    );
    drop(repo);
    drop(rt); // clean close checkpoints the WAL

    let absolute = std::fs::canonicalize(&args.out).unwrap_or(args.out);
    println!("{}", absolute.display());
    ExitCode::SUCCESS
}
