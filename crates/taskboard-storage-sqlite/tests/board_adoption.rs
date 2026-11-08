// SPDX-License-Identifier: MIT OR Apache-2.0
//! `AdoptMovedBoard` on disk: parity with the in-memory definition, and
//! no old id left in any text column of any table.

mod common;

use std::collections::BTreeMap;

use proptest::prelude::*;
use sqlx::{Connection as _, Row as _};
use taskboard_domain::board_move::BoardRekey;
use taskboard_domain::persistence::{
    PersistedState, PersistenceAction, TaskRepository, apply_actions,
};
use taskboard_domain::test_support::proptest_config;
use taskboard_domain::test_support::state::persisted_state_strategy;
use taskboard_domain::{
    BoardId, ConnectorId, LabelId, RemoteBoardBinding, RemoteBoardId, StackId, TaskId,
};
use uuid::Uuid;

fn fresh(id: Uuid) -> Uuid {
    Uuid::from_u128(!id.as_u128())
}

fn full_rekey(state: &PersistedState, board: BoardId) -> BoardRekey {
    let stacks: BTreeMap<_, _> = state
        .stacks
        .values()
        .filter(|s| s.board == board)
        .map(|s| (s.id, StackId::from_uuid(fresh(s.id.as_uuid()))))
        .collect();
    let tasks = state
        .tasks
        .values()
        .filter(|t| stacks.contains_key(&t.stack))
        .map(|t| (t.id, TaskId::from_uuid(fresh(t.id.as_uuid()))))
        .collect();
    let labels = state
        .labels
        .values()
        .filter(|l| l.board == board)
        .map(|l| (l.id, LabelId::from_uuid(fresh(l.id.as_uuid()))))
        .collect();
    let new = fresh(board.as_uuid());
    BoardRekey {
        old_board: board,
        new_board: BoardId::from_uuid(new),
        binding: RemoteBoardBinding {
            connector: ConnectorId::from("files-sync"),
            board: RemoteBoardId::files(new),
        },
        stacks,
        tasks,
        labels,
    }
}

async fn text_cells(path: &std::path::Path) -> (Vec<String>, usize) {
    let url = format!("sqlite://{}", path.display());
    let mut conn = sqlx::SqliteConnection::connect(&url)
        .await
        .expect("connects");
    let tables: Vec<String> = sqlx::query(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite%' AND name NOT LIKE '_sqlx%'",
    )
    .fetch_all(&mut conn)
    .await
    .expect("tables")
    .iter()
    .map(|row| row.get(0))
    .collect();
    let mut cells = Vec::new();
    for table in tables {
        let columns: Vec<String> =
            sqlx::query(sqlx::AssertSqlSafe(format!("PRAGMA table_info({table})")))
                .fetch_all(&mut conn)
                .await
                .expect("columns")
                .iter()
                .map(|row| row.get(1))
                .collect();
        for column in columns {
            // Titles and notes may legally hold arbitrary text.
            if matches!(
                column.as_str(),
                "title" | "description" | "text" | "task_title"
            ) {
                continue;
            }
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "SELECT CAST(\"{column}\" AS TEXT) FROM {table} WHERE \"{column}\" IS NOT NULL"
            )))
            .fetch_all(&mut conn)
            .await
            .expect("cells");
            cells.extend(
                rows.iter()
                    .map(|row| format!("{table}.{column}={}", row.get::<String, _>(0))),
            );
        }
    }
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut conn)
        .await
        .expect("fk check")
        .len();
    (cells, violations)
}

proptest! {
    #![proptest_config(proptest_config(12))]

    #[test]
    fn adoption_on_disk_matches_memory_and_leaves_no_old_id(state in persisted_state_strategy()) {
        let Some(board) = state.boards.keys().next().copied() else { return Ok(()); };
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("runtime").block_on(async {
            let db = common::FileDb::new().await;
            db.repo().apply(common::actions_from_state(&state)).await.expect("seeds");
            let seeded = db.repo().load().await.expect("loads");
            let rekey = full_rekey(&seeded, board);
            let old: Vec<String> = rekey.pairs().iter().map(|(o, _)| o.to_string()).collect();
            let action = vec![PersistenceAction::AdoptMovedBoard(Box::new(rekey))];

            let mut expected = seeded.clone();
            apply_actions(&mut expected, &action);
            db.repo().apply(action).await.expect("adopts");

            let loaded = db.repo().load().await.expect("reloads");
            assert_eq!(loaded, expected);
            let (cells, violations) = text_cells(db.path()).await;
            assert_eq!(violations, 0);
            for cell in &cells {
                assert!(!old.iter().any(|id| cell.contains(id.as_str())), "left behind: {cell}");
            }
        });
    }
}
