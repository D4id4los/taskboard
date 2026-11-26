// SPDX-License-Identifier: AGPL-3.0-only
//! The `find` verb's engine-seam suite (#1367 residual gap b): the
//! REPL/session wrapper `find_on` reads a real booted core's published
//! state — the pure projection below it is the lib unit tests' job
//! (`commands::find::tests`), never duplicated here.

use crate::common;

use common::tempdir;
use taskboard_app::bootstrap::{BootIdentity, bootstrap_in};
use taskboard_app::commands::find::{find_on, project};
use taskboard_app::notifications::LogSink;
use taskboard_app::secrets::EnvCredentialStore;

/// A standalone booted core over a tempdir db (the b-series fixture
/// shape): no server, no credentials consulted.
async fn boot(dir: &std::path::Path) -> taskboard_app::App {
    let config = common::standalone_config(dir, "find.sqlite");
    bootstrap_in(
        config,
        std::sync::Arc::new(EnvCredentialStore),
        std::sync::Arc::new(LogSink),
        BootIdentity::At(dir.to_path_buf()),
    )
    .await
    .expect("bootstrap")
}

/// `find_on` resolves the engine seam (`shared_state().load_full()`) and
/// projects through the same pure layer: a board created through the
/// typed command path is found by title and carries the typed row
/// (kind `board`, uuid id, `None` context). Fails-when-broken: a seam
/// regression that reads a stale/empty snapshot or bypasses the
/// projection loses the row.
#[tokio::test]
async fn find_on_reads_a_booted_core_and_projects_typed_rows() {
    let dir = tempdir();
    let app = boot(dir.path()).await;

    let receipt = taskboard_app::commands::boards::create_on(
        &app,
        "harbor".to_owned(),
        taskboard_domain::Color::new(""),
        String::new(),
    )
    .await
    .expect("board create on the booted core");

    let rows = find_on(&app, "harbor", None, None);
    assert_eq!(rows.len(), 1, "the created board is found");
    assert_eq!(rows[0].kind, "board");
    assert_eq!(rows[0].id, receipt.id);
    assert_eq!(rows[0].title, "harbor");
    assert_eq!(rows[0].context, None);

    // The same snapshot projects identically through the pure layer —
    // the seam adds the engine read, nothing else.
    let state = app.engine.shared_state().load_full();
    assert_eq!(project(&state, "harbor", None, None), rows);

    let outcome = app.shutdown().await;
    assert!(!outcome.actor_aborted);
}
