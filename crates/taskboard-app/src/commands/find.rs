// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard find` (#348, chunk 9): the scripting bridge over the
//! domain ranking core. The verb is the unfiltered lens — kind filtering
//! and the limit are projection concerns here, matching stays in
//! [`taskboard_domain::find`]. Infallible after boot: no hits is the
//! empty table (exit 0), never an error.

use std::sync::Arc;

use serde::Serialize;
use taskboard_domain::{AppState, SearchTarget};

use crate::cli::FindType;
use crate::commands::CommandError;
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// One row of `find` output: the hit kind, the jump id, the title, and
/// the owning context (`"<stack> › <board>"` for tasks, the board title
/// for stacks, none for boards). A dangling parent renders `None` —
/// data degradation, not an error. No quality column: ranking stays
/// inside the domain (design D-V9).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FindRow {
    /// `"task"`, `"stack"`, or `"board"`.
    pub kind: String,
    /// The entity's local uuid (the jump target).
    pub id: String,
    /// The matched title.
    pub title: String,
    /// The owning context line, when both parents resolve.
    pub context: Option<String>,
}

/// Runs `find` over a freshly booted core.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs (boot/config failures
/// only; the search itself never fails).
pub async fn find(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    query: &str,
    kind: Option<FindType>,
    limit: Option<u64>,
) -> Result<Vec<FindRow>, CommandError> {
    let query = query.to_owned();
    crate::commands::tasks::with_core(config, store, async move |app| {
        Ok(find_on(app, &query, kind, limit))
    })
    .await
}

/// The `find` steps on a booted core (the REPL/session seam).
pub(crate) fn find_on(
    app: &crate::App,
    query: &str,
    kind: Option<FindType>,
    limit: Option<u64>,
) -> Vec<FindRow> {
    let state = app.engine.shared_state().load_full();
    project(&state, query, kind, limit)
}

/// Ranks, filters, truncates, and projects one snapshot. The domain
/// owns matching and order; this only applies the caller's lenses
/// (kind after rank, limit last) and resolves context lines.
pub(crate) fn project(
    state: &AppState,
    query: &str,
    kind: Option<FindType>,
    limit: Option<u64>,
) -> Vec<FindRow> {
    taskboard_domain::find(state, query)
        .into_iter()
        .filter(|hit| kind_filter(hit.target, kind))
        .take(usize::try_from(limit.unwrap_or(u64::MAX)).unwrap_or(usize::MAX))
        .map(|hit| row_for(state, hit.target))
        .collect()
}

/// `None` keeps every kind; `Some(kind)` keeps its kind only.
fn kind_filter(target: SearchTarget, kind: Option<FindType>) -> bool {
    let target_kind = match target {
        SearchTarget::Task(_) => FindType::Task,
        SearchTarget::Stack(_) => FindType::Stack,
        SearchTarget::Board(_) => FindType::Board,
    };
    kind.is_none_or(|kind| kind == target_kind)
}

/// Resolves one hit against the snapshot (the row fields are the
/// projection contract; ranking state never crosses).
fn row_for(state: &AppState, target: SearchTarget) -> FindRow {
    match target {
        SearchTarget::Task(id) => {
            let (kind, id_str, title) = state.tasks.get(&id).map_or_else(
                || ("task", id.as_uuid().to_string(), String::new()),
                |task| ("task", task.id.as_uuid().to_string(), task.title.clone()),
            );
            let context = state
                .tasks
                .get(&id)
                .and_then(|task| state.stacks.get(&task.stack))
                .and_then(|stack| {
                    state
                        .boards
                        .get(&stack.board)
                        .map(|board| format!("{} › {}", stack.title, board.title))
                });
            FindRow {
                kind: kind.to_owned(),
                id: id_str,
                title,
                context,
            }
        }
        SearchTarget::Stack(id) => {
            let (id_str, title, context) = state.stacks.get(&id).map_or_else(
                || (id.as_uuid().to_string(), String::new(), None),
                |stack| {
                    (
                        stack.id.as_uuid().to_string(),
                        stack.title.clone(),
                        state.boards.get(&stack.board).map(|b| b.title.clone()),
                    )
                },
            );
            FindRow {
                kind: "stack".to_owned(),
                id: id_str,
                title,
                context,
            }
        }
        SearchTarget::Board(id) => {
            let (id_str, title) = state.boards.get(&id).map_or_else(
                || (id.as_uuid().to_string(), String::new()),
                |board| (board.id.as_uuid().to_string(), board.title.clone()),
            );
            FindRow {
                kind: "board".to_owned(),
                id: id_str,
                title,
                context: None,
            }
        }
    }
}

/// The human-readable find listing: `kind<TAB>id<TAB>title<TAB>context`
/// (the context column empty for boards); the empty render is the
/// no-hits line, not an error.
#[must_use]
pub fn render_text_list(rows: &[FindRow]) -> String {
    if rows.is_empty() {
        return "no hits".to_owned();
    }
    rows.iter()
        .map(|r| {
            format!(
                "{}\t{}\t{}\t{}",
                r.kind,
                r.id,
                r.title,
                r.context.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use taskboard_domain::ids::{StackId, TaskId};
    use taskboard_domain::{Board, Stack, Task, TaskClocks};

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    fn id<T: From<uuid::Uuid>>(n: u64) -> T {
        T::from(uuid::Uuid::from_u64_pair(0, n))
    }

    fn board(n: u64, title: &str) -> Board {
        Board {
            id: id(n),
            remote: None,
            title: title.to_owned(),
            color: taskboard_domain::Color::new("#000000"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn stack(n: u64, board: u64, title: &str) -> Stack {
        Stack {
            id: id(n),
            remote: None,
            board: id(board),
            title: title.to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn task(n: u64, stack: u64, title: &str) -> Task {
        Task {
            id: id(n),
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: id(stack),
            order: 0,
            label_memberships: std::collections::BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: epoch(),
                description: epoch(),
                duedate: epoch(),
                scheduled: epoch(),
                done: epoch(),
                position: epoch(),
                effort: epoch(),
                importance: epoch(),
                urgency: epoch(),
                recur: epoch(),

                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn state() -> AppState {
        let mut state = AppState::default();
        for b in [board(1, "alpha"), board(2, "beta")] {
            state.boards.insert(b.id, b);
        }
        for s in [stack(10, 1, "todo"), stack(11, 2, "done")] {
            state.stacks.insert(s.id, s);
        }
        for t in [
            task(20, 10, "write alpha"),
            task(21, 11, "alpha retrospective"),
        ] {
            state.tasks.insert(t.id, t);
        }
        state
    }

    #[test]
    fn ranking_order_and_context_projection() {
        let state = state();
        let rows = project(&state, "alpha", None, None);
        // Exact ("alpha" the board) first; then the Prefix task hit
        // ("alpha retrospective"); then the WordStart task hit
        // ("write alpha" — the boundary after the space).
        assert_eq!(rows[0].kind, "board");
        assert_eq!(rows[0].context, None);
        assert_eq!(
            rows[1],
            FindRow {
                kind: "task".to_owned(),
                id: id::<TaskId>(21).as_uuid().to_string(),
                title: "alpha retrospective".to_owned(),
                context: Some("done \u{203a} beta".to_owned()),
            }
        );
        assert_eq!(
            rows[2],
            FindRow {
                kind: "task".to_owned(),
                id: id::<TaskId>(20).as_uuid().to_string(),
                title: "write alpha".to_owned(),
                context: Some("todo \u{203a} alpha".to_owned()),
            }
        );
    }

    #[test]
    fn kind_filter_and_limit_apply_after_ranking() {
        let state = state();
        let rows = project(&state, "alpha", Some(FindType::Task), None);
        assert_eq!(
            rows.iter().map(|r| r.kind.as_str()).collect::<Vec<_>>(),
            ["task", "task"]
        );
        // Limit truncates the ranked list, not the match set.
        let limited = project(&state, "alpha", None, Some(2));
        assert_eq!(limited.len(), 2);
        assert_eq!(limited, project(&state, "alpha", None, None)[..2].to_vec());
        // A limit of 1 keeps only the exact hit.
        let one = project(&state, "alpha", None, Some(1));
        assert_eq!(one[0].kind, "board");
    }

    #[test]
    fn empty_query_projects_no_rows_and_renders_the_no_hits_line() {
        let state = state();
        assert_eq!(project(&state, "", None, None), Vec::new());
        assert_eq!(project(&state, "   ", None, None), Vec::new());
        assert_eq!(render_text_list(&[]), "no hits");
    }

    #[test]
    fn dangling_parent_renders_context_none() {
        let mut state = state();
        // The board under task 21's stack vanishes (a data-degradation
        // posture, not an error): the row survives with context `None`.
        let stack_board = {
            let stack = state.stacks.get(&id::<StackId>(11)).expect("stack exists");
            stack.board
        };
        state.boards.remove(&stack_board);
        let rows = project(&state, "retrospective", None, None);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].context, None);
    }
}
