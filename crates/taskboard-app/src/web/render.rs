// SPDX-License-Identifier: AGPL-3.0-only
//! The dashboard's pure render handlers (K11 design §7 rule 2):
//! `(request metadata, AppState snapshot, EngineMetricsSnapshot, render
//! params) → response`. No `Arc` escapes into rendering, no I/O, no
//! wall-clock reads — the render moment enters as [`RenderCtx::now`],
//! so equal inputs produce byte-identical bytes (the `insta`
//! snapshots' contract) and the server never subscribes to anything.

use std::borrow::Cow;

use crate::config::SyncBackend;
use chrono::{DateTime, Utc};
use taskboard_domain::{AppState, BoardId, SyncPhase};

use taskboard_state::EngineMetricsSnapshot;

/// `write!` into a `String` cannot fail; the `fmt::Result` plumbing is
/// discarded at this one helper instead of threading an impossible
/// error through the renderers.
macro_rules! html {
    ($body:expr, $($arg:tt)*) => {{
        let _ = ::std::fmt::Write::write_fmt(&mut $body, ::std::format_args!($($arg)*));
    }};
}

/// The versioned stylesheet asset (`/style-v1.css`; a redesign ships
/// v2 — the URL version is the whole cache-busting story, Q8).
pub const STYLESHEET: &str = include_str!("assets/style-v1.css");
/// The single allowed script (Q7 item 4): the ES5 reconnect enhancement,
/// static text — never generated, templated, or configurable.
pub const RECONNECT_SCRIPT: &str = include_str!("assets/reconnect.js");

/// Everything a render needs besides the snapshots: the URL space's
/// token, the meta-refresh cadence, the typed backend/mode words, the
/// app version, and the render moment (injected, never read from the
/// wall — determinism is the snapshots' contract).
#[derive(Debug, Clone)]
pub struct RenderCtx {
    /// The capability token — the pages' own links carry it.
    pub token: String,
    /// Meta-refresh cadence in seconds (config-validated ≥ 5).
    pub refresh_secs: u64,
    /// The typed sync backend (Q6: standalone is rendered *from this*,
    /// never inferred from empty counters).
    pub backend: SyncBackend,
    /// The lifecycle word (`daemon` / `kiosk appliance`).
    pub mode: &'static str,
    /// The app version.
    pub version: &'static str,
    /// The frozen render moment (overdue classification only).
    pub now: DateTime<Utc>,
}

/// Cache-Control policy (Q8): dynamic HTML never caches, the
/// versioned stylesheet caches for a year. No ETag/Last-Modified
/// machinery exists in this lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    /// `Cache-Control: no-store` — the dynamic pages.
    NoStore,
    /// `Cache-Control: max-age=31536000` — the versioned stylesheet.
    OneYear,
}

impl CachePolicy {
    /// The header value.
    #[must_use]
    pub fn value(self) -> &'static str {
        match self {
            Self::NoStore => "no-store",
            Self::OneYear => "max-age=31536000",
        }
    }
}

/// A handler's complete answer: everything the HTTP layer needs to
/// build the hyper response, decided here so the server stays a dumb
/// transporter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// HTTP status (`200`/`404` in this lane).
    pub status: u16,
    /// `Content-Type` header value.
    pub content_type: &'static str,
    /// The cache policy.
    pub cache: CachePolicy,
    /// The body bytes (HTML or CSS).
    pub body: String,
}

/// The index page: app identity, the typed sync-backend section, the
/// aggregate sync health, the engine counters, and one sync-health row
/// (plus link) per live board.
#[must_use]
pub fn render_index(state: &AppState, metrics: EngineMetricsSnapshot, ctx: &RenderCtx) -> Rendered {
    let aggregate = state.sync.aggregate();
    let mut body = String::with_capacity(4096);
    body.push_str(&page_head(
        "taskboard",
        ctx,
        Some("<link rel=\"stylesheet\" href=\"style-v1.css\">"),
    ));
    body.push_str("<h1>taskboard</h1>\n");
    html!(
        body,
        "<p class=\"sub\">version {} · {} · sync backend: {}</p>\n",
        esc(ctx.version),
        esc(ctx.mode),
        backend_word(ctx.backend),
    );

    body.push_str("<section id=\"sync\"><h2>sync</h2>\n");
    html!(
        body,
        "<p>phase: {}</p>\n<p>last success: {}</p>\n<p>pending ops: {}</p>\n",
        phase_word(aggregate.phase),
        instant_word(aggregate.last_success),
        aggregate.pending_ops,
    );
    body.push_str("</section>\n");

    body.push_str("<section id=\"engine\"><h2>engine</h2>\n");
    html!(
        body,
        "<p>commands: {} · rejections: {} · signals: {}</p>\n",
        metrics.commands,
        metrics.rejections,
        metrics.signals,
    );
    body.push_str("</section>\n");

    body.push_str("<section id=\"boards\"><h2>boards</h2>\n");
    let live = live_boards(state);
    if live.is_empty() {
        body.push_str("<p>no boards</p>\n");
    } else {
        body.push_str(
            "<table><tr><th>board</th><th>phase</th><th>pending</th>\
<th>cycles</th><th>pushes</th><th>pulls</th><th>failures</th><th>offline</th>\
<th>last cycle</th><th>last success</th></tr>\n",
        );
        for (id, board) in live {
            let status = state.sync.get(*id).cloned().unwrap_or_default();
            html!(
                body,
                "<tr><td><a href=\"board/{}\">{}</a></td><td>{}</td><td>{}</td>\
<td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
                id.as_uuid(),
                esc(&board.title),
                phase_word(status.phase),
                status.pending_ops,
                status.counters.cycles,
                status.counters.pushes,
                status.counters.pulls,
                status.counters.failures,
                status.counters.offline_episodes,
                duration_word(status.counters.last_cycle),
                instant_word(status.last_success),
            );
        }
        body.push_str("</table>\n");
    }
    body.push_str("</section>\n</body>\n</html>\n");
    Rendered {
        status: 200,
        content_type: "text/html; charset=utf-8",
        cache: CachePolicy::NoStore,
        body,
    }
}

/// One board's page: its stacks as columns (CSS tables — Q7 item 2),
/// its cards as title + due + done state. `None` when the board id is
/// unknown (or tombstoned/archived) — the caller's 404, uniformly.
#[must_use]
pub fn render_board(state: &AppState, board_id: BoardId, ctx: &RenderCtx) -> Option<Rendered> {
    let board = state.boards.get(&board_id)?;
    if board.deleted || board.archived {
        return None;
    }
    let status = state.sync.get(board_id).cloned().unwrap_or_default();
    let mut stacks: Vec<&taskboard_domain::Stack> = state
        .stacks
        .values()
        .filter(|stack| stack.board == board_id && !stack.deleted && !stack.archived)
        .collect();
    stacks.sort_by_key(|stack| (stack.order, stack.id));

    let mut body = String::with_capacity(4096);
    body.push_str(&page_head(
        &board.title,
        ctx,
        Some("<link rel=\"stylesheet\" href=\"../style-v1.css\">"),
    ));
    html!(
        body,
        "<h1>{}</h1>\n<p class=\"sub\">phase: {} · pending ops: {} · \
last success: {}</p>\n",
        esc(&board.title),
        phase_word(status.phase),
        status.pending_ops,
        instant_word(status.last_success),
    );
    body.push_str("<div class=\"board\">\n");
    for stack in stacks {
        html!(
            body,
            "<div class=\"stack\"><h2>{}</h2>\n",
            esc(&stack.title)
        );
        let mut tasks: Vec<&taskboard_domain::Task> = state
            .tasks
            .values()
            .filter(|task| task.stack == stack.id && task.is_live() && !task.archived)
            .collect();
        tasks.sort_by_key(|task| (task.order, task.id));
        for task in tasks {
            let done = task.done.is_some();
            let overdue = task.duedate.is_some_and(|due| due < ctx.now) && !done;
            let class = match (done, overdue) {
                (true, _) => "card done",
                (false, true) => "card overdue",
                (false, false) => "card",
            };
            html!(body, "<div class=\"{class}\">");
            html!(body, "<p class=\"title\">{}</p>", esc(&task.title));
            if let Some(due) = task.duedate {
                html!(
                    body,
                    "<p class=\"due\">due {}</p>",
                    esc(&due.format("%Y-%m-%d %H:%M").to_string())
                );
            }
            body.push_str("</div>\n");
        }
        body.push_str("</div>\n");
    }
    body.push_str("</div>\n</body>\n</html>\n");
    Some(Rendered {
        status: 200,
        content_type: "text/html; charset=utf-8",
        cache: CachePolicy::NoStore,
        body,
    })
}

/// The stylesheet response (the one-year-cached arm).
#[must_use]
pub fn render_stylesheet() -> Rendered {
    Rendered {
        status: 200,
        content_type: "text/css; charset=utf-8",
        cache: CachePolicy::OneYear,
        body: STYLESHEET.to_owned(),
    }
}

/// The uniform 404 (wrong token, unknown path, unknown board — all the
/// same answer; the capability discipline).
#[must_use]
pub fn render_not_found() -> Rendered {
    Rendered {
        status: 404,
        content_type: "text/plain; charset=utf-8",
        cache: CachePolicy::NoStore,
        body: String::new(),
    }
}

/// The 405 for a non-GET on a valid path (with `Allow`, per RFC 9110).
#[must_use]
pub fn render_method_not_allowed() -> Rendered {
    Rendered {
        status: 405,
        content_type: "text/plain; charset=utf-8",
        cache: CachePolicy::NoStore,
        body: String::new(),
    }
}

/// The live, non-archived boards in deterministic id order.
fn live_boards(state: &AppState) -> Vec<(&BoardId, &taskboard_domain::Board)> {
    state
        .boards
        .iter()
        .filter(|(_, board)| !board.deleted && !board.archived)
        .collect()
}

/// The document head through `<body>`: HTML5 doctype, charset, viewport
/// (Q7 item 1), the meta-refresh baseline (item 5), and — as the single
/// `<script>` in the page — the ES5 reconnect enhancement (item 4).
fn page_head(title: &str, ctx: &RenderCtx, stylesheet: Option<&str>) -> String {
    let mut head = String::with_capacity(2048);
    head.push_str("<!doctype html>\n<html>\n<head>\n");
    head.push_str("<meta charset=\"utf-8\">\n");
    head.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html!(head, "<title>{}</title>\n", esc(title));
    html!(
        head,
        "<meta http-equiv=\"refresh\" content=\"{}\">\n",
        ctx.refresh_secs
    );
    if let Some(link) = stylesheet {
        head.push_str(link);
        head.push('\n');
    }
    head.push_str("<script>\n");
    head.push_str(RECONNECT_SCRIPT);
    head.push_str("</script>\n");
    head.push_str("</head>\n<body>\n");
    head
}

/// The typed backend word (Q6: typed from [`SyncBackend`], never
/// inferred from empty counters).
fn backend_word(backend: SyncBackend) -> &'static str {
    match backend {
        SyncBackend::Standalone => "standalone",
        SyncBackend::Nextcloud => "Nextcloud",
    }
}

/// The phase word, offline included (A4: rendered from the same
/// snapshot — no separate offline page, no separate state source).
fn phase_word(phase: SyncPhase) -> String {
    match phase {
        SyncPhase::Idle => "idle".to_owned(),
        SyncPhase::Syncing => "syncing".to_owned(),
        SyncPhase::Offline => "offline".to_owned(),
        SyncPhase::Failed { last_error } => format!("failed ({})", error_word(last_error)),
    }
}

fn error_word(kind: taskboard_domain::SyncErrorKind) -> &'static str {
    use taskboard_domain::SyncErrorKind as K;
    match kind {
        K::Network => "network",
        K::Auth => "auth",
        K::Forbidden => "forbidden",
        K::Server => "server",
        K::BadRequest => "bad-request",
        K::LocalData => "local-data",
        K::NoBoard => "no-board",
    }
}

/// `None` renders as the em-dash placeholder (no wall clock involved).
fn instant_word(instant: Option<DateTime<Utc>>) -> String {
    instant.map_or_else(
        || "—".to_owned(),
        |at| at.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
    )
}

fn duration_word(duration: Option<std::time::Duration>) -> String {
    duration.map_or_else(|| "—".to_owned(), |d| format!("{d:?}"))
}

/// The one escaping definition for both pages: `& < > " '` only.
fn esc(raw: &str) -> Cow<'_, str> {
    if !raw
        .bytes()
        .any(|b| matches!(b, b'&' | b'<' | b'>' | b'"' | b'\''))
    {
        return Cow::Borrowed(raw);
    }
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::{
        CachePolicy, RenderCtx, Rendered, render_board, render_index, render_not_found,
        render_stylesheet,
    };
    use crate::config::SyncBackend;
    use taskboard_domain::{
        AppState, Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, SyncErrorKind,
        SyncPhase, SyncStates, Task, TaskClocks,
    };
    use taskboard_state::EngineMetricsSnapshot;

    fn ctx() -> RenderCtx {
        RenderCtx {
            token: "tok".to_owned(),
            refresh_secs: 30,
            backend: SyncBackend::Nextcloud,
            mode: "daemon",
            version: "0.1.0",
            now: chrono::DateTime::from_timestamp(1_800_000_000, 0).expect("fixture now"),
        }
    }

    fn board(id: u128, title: &str) -> Board {
        Board {
            id: BoardId::from_uuid(uuid::Uuid::from_u128(id)),
            remote: None,
            title: title.to_owned(),
            color: Color::new("3366cc"),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            remote_seen: None,
        }
    }

    fn stack(id: u128, board: u128, title: &str, order: u64) -> Stack {
        Stack {
            id: StackId::from_uuid(uuid::Uuid::from_u128(id)),
            remote: None,
            board: BoardId::from_uuid(uuid::Uuid::from_u128(board)),
            title: title.to_owned(),
            order: i64::try_from(order).expect("fixture order fits"),
            archived: false,
            deleted: false,
            clocks: fixed_stack_clocks(),
            remote_seen: None,
        }
    }

    fn task(id: u128, stack: u128, title: &str, order: u64) -> Task {
        Task {
            id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(id)),
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            done: None,
            stack: StackId::from_uuid(uuid::Uuid::from_u128(stack)),
            order: i64::try_from(order).expect("fixture order fits"),
            labels: std::collections::BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: task_clocks(),
            remote_seen: None,
        }
    }

    fn fixed_stack_clocks() -> StackClocks {
        let t = chrono::DateTime::from_timestamp(1_600_000_000, 0).expect("fixture ts");
        StackClocks {
            title: t,
            order: t,
            deleted: t,
        }
    }

    /// One fixed stamp for every clock (the fixtures are deterministic;
    /// the clocks are payload this lane never reads).
    fn task_clocks() -> TaskClocks {
        let t = chrono::DateTime::from_timestamp(1_600_000_000, 0).expect("fixture ts");
        TaskClocks {
            title: t,
            description: t,
            duedate: t,
            done: t,
            position: t,
            labels: t,
            archived: t,
            deleted: t,
        }
    }

    fn metrics() -> EngineMetricsSnapshot {
        EngineMetricsSnapshot {
            commands: 7,
            rejections: 1,
            signals: 9,
        }
    }

    /// Handler purity (§7 rule 2): the same snapshots in, byte-identical
    /// responses out.
    #[test]
    fn handlers_are_pure_over_their_inputs() {
        let state = AppState::default();
        let c = ctx();
        let first = render_index(&state, metrics(), &c);
        let second = render_index(&state, metrics(), &c);
        assert_eq!(first, second);
        assert_eq!(
            render_board(&state, BoardId::from_uuid(uuid::Uuid::nil()), &c),
            render_board(&state, BoardId::from_uuid(uuid::Uuid::nil()), &c)
        );
    }

    /// Q7 item 4: the reconnect enhancement is the *only* script on
    /// every page, and the meta-refresh baseline is present regardless —
    /// the no-JS-required guarantee is structural.
    #[test]
    fn every_page_is_scriptless_but_for_the_reconnect_enhancement() {
        let c = ctx();
        let empty = AppState::default();
        let mut populated = AppState::default();
        populated.boards.insert(
            BoardId::from_uuid(uuid::Uuid::from_u128(1)),
            board(1, "one"),
        );
        for page in [
            render_index(&empty, metrics(), &c),
            render_index(&populated, metrics(), &c),
            render_board(&populated, BoardId::from_uuid(uuid::Uuid::from_u128(1)), &c)
                .expect("a live board renders"),
        ] {
            assert_eq!(
                page.body.match_indices("<script").count(),
                1,
                "exactly one script element"
            );
            assert!(
                page.body.contains(super::RECONNECT_SCRIPT),
                "the script is the static ES5 asset"
            );
            assert!(
                page.body
                    .contains("<meta http-equiv=\"refresh\" content=\"30\">"),
                "the meta-refresh baseline survives with the script"
            );
            assert!(page.body.starts_with("<!doctype html>"));
            assert!(page.body.contains("<meta charset=\"utf-8\">"));
            assert!(page.body.contains("name=\"viewport\""));
        }
    }

    /// Q7 item 6/7: the stylesheet's modesty rules are greppable — no
    /// grid, no flex reliance, no viewport units, no calc.
    #[test]
    fn the_stylesheet_stays_on_the_2014_checklist() {
        let css = super::STYLESHEET;
        for banned in ["display: grid", "display:grid", "flex", "vh", "vw", "calc("] {
            assert!(!css.contains(banned), "{banned} must not appear");
        }
        for required in ["@media", "border-radius", "box-shadow"] {
            assert!(css.contains(required), "{required} must appear");
        }
    }

    /// Q7 item 8: the page-weight budget at board scale — five stacks
    /// of forty long-titled cards must fit ~50 KB.
    #[test]
    fn a_board_scale_page_fits_the_weight_budget() {
        let mut state = AppState::default();
        let board_id = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        state.boards.insert(board_id, board(1, "budget"));
        let title = "a fairly long card title for the weight budget ";
        for stack_index in 0u128..5 {
            let stack_id = StackId::from_uuid(uuid::Uuid::from_u128(100 + stack_index));
            state.stacks.insert(
                stack_id,
                stack(
                    100 + stack_index,
                    1,
                    "column",
                    u64::try_from(stack_index).expect("fixture index fits"),
                ),
            );
            for card_index in 0u128..40 {
                let mut name = title.to_owned();
                name.push_str(&card_index.to_string());
                let tid = 1_000 + stack_index * 100 + card_index;
                state.tasks.insert(
                    taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(tid)),
                    task(
                        tid,
                        100 + stack_index,
                        &name,
                        u64::try_from(card_index).expect("fixture index fits"),
                    ),
                );
            }
        }
        let page = render_board(&state, board_id, &ctx()).expect("renders");
        assert!(
            page.body.len() <= 50 * 1024,
            "board page must fit the ~50 KB budget, got {} bytes",
            page.body.len()
        );
    }

    /// The snapshots: empty standalone index, populated index, board
    /// with done and overdue cards, offline phase. The HTML is the UI
    /// state of this lane — the engine-snapshot discipline applied to
    /// the web renderer.
    #[test]
    fn snapshot_index_standalone_empty() {
        let state = AppState::default();
        let mut c = ctx();
        c.backend = SyncBackend::Standalone;
        let page = render_index(
            &state,
            EngineMetricsSnapshot {
                commands: 0,
                rejections: 0,
                signals: 0,
            },
            &c,
        );
        insta::assert_snapshot!("index_standalone_empty", page.body);
    }

    #[test]
    fn snapshot_index_populated_nextcloud() {
        let mut state = AppState::default();
        let id = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        state.boards.insert(id, board(1, "Household <& chores>"));
        state.sync = SyncStates::new();
        state.sync.set_stored(
            id,
            SyncPhase::Idle,
            Some(chrono::DateTime::from_timestamp(1_700_000_000, 0).expect("fixture ts")),
        );
        state.sync.get_mut(id).expect("row").pending_ops = 3;
        let page = render_index(&state, metrics(), &ctx());
        insta::assert_snapshot!("index_populated", page.body);
    }

    #[test]
    fn snapshot_board_page_with_done_and_overdue_cards() {
        let mut state = AppState::default();
        let board_id = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        let stack_id = StackId::from_uuid(uuid::Uuid::from_u128(10));
        state.boards.insert(board_id, board(1, "Renovation"));
        state.stacks.insert(stack_id, stack(10, 1, "To do", 0));
        let mut open = task(100, 10, "sand the door", 0);
        open.duedate = Some(ctx().now - chrono::Duration::days(2));
        let mut finished = task(101, 10, "order paint", 1);
        finished.done = Some(ctx().now - chrono::Duration::days(1));
        state.tasks.insert(open.id, open);
        state.tasks.insert(finished.id, finished);
        state.sync = SyncStates::new();
        state.sync.set_stored(board_id, SyncPhase::Offline, None);
        let page = render_board(&state, board_id, &ctx()).expect("renders");
        insta::assert_snapshot!("board_done_overdue_offline", page.body);
    }

    #[test]
    fn snapshot_index_failed_phase_and_counter_columns() {
        let mut state = AppState::default();
        let id = BoardId::from_uuid(uuid::Uuid::from_u128(2));
        state.boards.insert(id, board(2, "work"));
        state.sync = SyncStates::new();
        state.sync.set_stored(
            id,
            SyncPhase::Failed {
                last_error: SyncErrorKind::Network,
            },
            None,
        );
        let row = state.sync.get_mut(id).expect("row");
        row.counters.cycles = 12;
        row.counters.pushes = 30;
        row.counters.pulls = 44;
        row.counters.failures = 2;
        row.counters.offline_episodes = 1;
        row.counters.last_cycle = Some(std::time::Duration::from_millis(1530));
        let page = render_index(&state, metrics(), &ctx());
        insta::assert_snapshot!("index_failed_counters", page.body);
    }

    /// The cache policy attachment is part of the handler's answer
    /// (Q8): dynamic HTML never caches; the stylesheet caches a year.
    #[test]
    fn cache_policies_attach_per_kind() {
        assert_eq!(render_stylesheet().cache, CachePolicy::OneYear);
        assert_eq!(CachePolicy::OneYear.value(), "max-age=31536000");
        assert_eq!(CachePolicy::NoStore.value(), "no-store");
        let empty: Rendered = render_not_found();
        assert_eq!(empty.cache, CachePolicy::NoStore);
        assert_eq!(empty.status, 404);
    }

    /// Escaping: board and card titles are the only user-controlled
    /// strings on the pages.
    #[test]
    fn titles_are_escaped() {
        let mut state = AppState::default();
        let id = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        state
            .boards
            .insert(id, board(1, "<script>alert(1)</script>"));
        let page = render_index(&state, metrics(), &ctx());
        assert!(!page.body.contains("<script>alert(1)</script>"));
        assert!(page.body.contains("&lt;script&gt;"));
    }

    /// An unknown (or tombstoned) board renders no board page.
    #[test]
    fn an_unknown_board_is_none() {
        let state = AppState::default();
        assert!(render_board(&state, BoardId::from_uuid(uuid::Uuid::nil()), &ctx()).is_none());
    }
}
