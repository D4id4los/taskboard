// SPDX-License-Identifier: MIT OR Apache-2.0
//! C1/C2 — the domain contract harness run against the sqlite adapter,
//! on `:memory:` and on a file-backed database (the plan's Phase 2 exit
//! criterion).

use taskboard_domain::test_support::contract::assert_task_repository_contract_async;
use taskboard_storage_sqlite::{open, open_memory};

#[tokio::test]
async fn sqlite_memory_repository_satisfies_contract() {
    let repo = open_memory().await.expect("open memory db");
    assert_task_repository_contract_async(&repo).await;
}

#[tokio::test]
async fn sqlite_file_repository_satisfies_contract() {
    let file = tempfile::Builder::new()
        .suffix(".db")
        .tempfile()
        .expect("tempfile");
    let repo = open(file.path()).await.expect("open file db");
    assert_task_repository_contract_async(&repo).await;
}
