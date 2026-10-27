// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fuzzy title search over an [`AppState`] (#348).
//!
//! The public surface is the entire contract, deliberately four items:
//! [`find`], [`SearchHit`], [`SearchTarget`], and [`MatchQuality`].
//! Everything else — the case folding, the class classifier, the
//! subsequence check, the ranking sort — is private and may be replaced
//! wholesale (e.g. by a fuzzy-matcher crate mapping its results into
//! [`MatchQuality`]) without any consumer noticing: this is the
//! crate-swap firewall (design D-V8). The in-module property suite
//! carries `contract-shaped` / `algorithm-shaped` tags so a swap knows
//! which tests must survive it.
//!
//! There is no numeric score anywhere: hits carry a [`MatchQuality`]
//! class, and ranking is the tuple `(quality, title_len, target)`
//! ascending — total, because within a kind ids are unique and across
//! kinds the [`SearchTarget`] variant tag discriminates.
//!
//! Scope (design D1/D-V8): live boards and stacks (`!deleted &&
//! !archived`) and `is_live()` tasks (`!deleted`; done and archived
//! tasks are hits). The function is convention-blind: no anchor-title
//! filtering, no view logic — hiding is presentation-side.

use crate::ids::{BoardId, StackId, TaskId};
use crate::state::AppState;

/// The kind of entity a hit points at. The declaration order is the
/// cross-kind tiebreak rank (`Task` < `Stack` < `Board`) and part of the
/// ranking contract: the mixed-kind sort key ends in the target itself,
/// which makes the order total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchTarget {
    /// A task card.
    Task(TaskId),
    /// A stack (column).
    Stack(StackId),
    /// A board.
    Board(BoardId),
}

/// How well a title matches the query. Declaration order is rank order:
/// ascending sorts put [`MatchQuality::Exact`] first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchQuality {
    /// The folded title equals the folded query.
    Exact,
    /// The folded title starts with the folded query.
    Prefix,
    /// The folded query matches at a word-start boundary (index > 0,
    /// immediately after any non-alphanumeric character).
    WordStart,
    /// The folded title contains the folded query elsewhere.
    Substring,
    /// The folded query's characters occur in the folded title in
    /// order (greedy, spread allowed).
    Subsequence,
}

/// One ranked hit: where to jump and how well it matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchHit {
    /// The entity to jump to.
    pub target: SearchTarget,
    /// The best match class the title attained.
    pub quality: MatchQuality,
}

/// Ranks every in-scope title against `query`: case-folded subsequence
/// classification, ordered by `(quality, title_len, target)` ascending.
///
/// Pure, total, deterministic: same state and query, same output, for
/// any insertion order (the state's maps are `BTreeMap`s). The query is
/// trimmed; empty-after-trim yields no hits — an empty box is not a
/// search. Matching is title-only over tasks, stacks, and boards
/// (descriptions and labels are out of scope, #348).
#[must_use]
pub fn find(state: &AppState, query: &str) -> Vec<SearchHit> {
    let folded_query = query.trim().to_lowercase();
    if folded_query.is_empty() {
        return Vec::new();
    }

    let mut ranked: Vec<(MatchQuality, usize, SearchTarget)> = Vec::new();
    for (id, board) in &state.boards {
        consider(
            &mut ranked,
            &board.title,
            SearchTarget::Board(*id),
            !board.deleted && !board.archived,
            &folded_query,
        );
    }
    for (id, stack) in &state.stacks {
        consider(
            &mut ranked,
            &stack.title,
            SearchTarget::Stack(*id),
            !stack.deleted && !stack.archived,
            &folded_query,
        );
    }
    for (id, task) in &state.tasks {
        consider(
            &mut ranked,
            &task.title,
            SearchTarget::Task(*id),
            task.is_live(),
            &folded_query,
        );
    }
    ranked.sort_by_key(|&(quality, title_len, target)| (quality, title_len, target));
    ranked
        .into_iter()
        .map(|(quality, _, target)| SearchHit { target, quality })
        .collect()
}

/// Classifies one title and, when it is both in scope and a hit,
/// records `(quality, title_len, target)`.
fn consider(
    ranked: &mut Vec<(MatchQuality, usize, SearchTarget)>,
    title: &str,
    target: SearchTarget,
    in_scope: bool,
    folded_query: &str,
) {
    if !in_scope {
        return;
    }
    let folded_title = title.to_lowercase();
    if let Some(quality) = classify(&folded_title, folded_query) {
        ranked.push((quality, title.chars().count(), target));
    }
}

/// The class ladder: best attainable class for an already-folded title
/// against an already-folded nonempty query, or no hit.
fn classify(folded_title: &str, folded_query: &str) -> Option<MatchQuality> {
    if folded_title == folded_query {
        return Some(MatchQuality::Exact);
    }
    if folded_title.starts_with(folded_query) {
        return Some(MatchQuality::Prefix);
    }
    if starts_at_word_boundary(folded_title, folded_query) {
        return Some(MatchQuality::WordStart);
    }
    if folded_title.contains(folded_query) {
        return Some(MatchQuality::Substring);
    }
    if is_subsequence(folded_title, folded_query) {
        return Some(MatchQuality::Subsequence);
    }
    None
}

/// `true` when the query matches at index > 0 straight after a
/// non-alphanumeric character (whitespace, punctuation, symbols, and
/// underscore all count as boundaries).
fn starts_at_word_boundary(folded_title: &str, folded_query: &str) -> bool {
    let boundary_at = |i: usize| {
        folded_title[..i]
            .chars()
            .next_back()
            .is_some_and(|prev| !prev.is_alphanumeric())
            && folded_title[i..].starts_with(folded_query)
    };
    folded_title
        .char_indices()
        .skip(1)
        .any(|(i, _)| boundary_at(i))
}

/// Greedy two-pointer: every query char occurs in the title, in order.
fn is_subsequence(folded_title: &str, folded_query: &str) -> bool {
    let mut title_chars = folded_title.chars();
    folded_query.chars().all(|q| title_chars.any(|t| t == q))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Board, Stack, Task};
    use crate::test_support::{proptest_config, string_strategy, task_strategy};
    use proptest::prelude::*;

    // ------------------------------------------------------------------
    // In-module builders (part of the suite that travels with a swap;
    // no board/stack strategies exist in test_support).
    // ------------------------------------------------------------------

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        crate::entities::BoardClocks::epoch()
    }

    fn board_flags(id: BoardId, title: &str, archived: bool, deleted: bool) -> Board {
        Board {
            id,
            remote: None,
            title: title.to_owned(),
            color: crate::entities::Color::new("#000000"),
            archived,
            deleted,
            clocks: crate::entities::BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn board(id: BoardId, title: &str) -> Board {
        board_flags(id, title, false, false)
    }

    fn stack_flags(
        id: StackId,
        board: BoardId,
        title: &str,
        archived: bool,
        deleted: bool,
    ) -> Stack {
        Stack {
            id,
            remote: None,
            board,
            title: title.to_owned(),
            order: 0,
            archived,
            deleted,
            clocks: crate::entities::StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn stack(id: StackId, board: BoardId, title: &str) -> Stack {
        stack_flags(id, board, title, false, false)
    }

    fn task(id: TaskId, title: &str) -> Task {
        Task {
            id,
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from(uuid::Uuid::nil()),
            order: 0,
            labels: std::collections::BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: epoch(),
                description: epoch(),
                duedate: epoch(),
                scheduled: epoch(),
                done: epoch(),
                position: epoch(),
                labels: epoch(),
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

    fn state_from(
        boards: Vec<Board>,
        stacks: Vec<Stack>,
        tasks: impl IntoIterator<Item = Task>,
    ) -> AppState {
        let mut state = AppState::default();
        for b in boards {
            state.boards.insert(b.id, b);
        }
        for s in stacks {
            state.stacks.insert(s.id, s);
        }
        for t in tasks {
            state.tasks.insert(t.id, t);
        }
        state
    }

    // Distinct fixed UUIDs from a counter (deterministic, order-stable).
    fn t(n: u64) -> TaskId {
        TaskId::from(uuid::Uuid::from_u64_pair(0, n))
    }

    fn s(n: u64) -> StackId {
        StackId::from(uuid::Uuid::from_u64_pair(1, n))
    }

    fn b(n: u64) -> BoardId {
        BoardId::from(uuid::Uuid::from_u64_pair(2, n))
    }

    fn hit_targets(hits: &[SearchHit]) -> Vec<SearchTarget> {
        hits.iter().map(|hit| hit.target).collect()
    }

    fn hit_qualities(hits: &[SearchHit]) -> Vec<MatchQuality> {
        hits.iter().map(|hit| hit.quality).collect()
    }

    // ------------------------------------------------------------------
    // Classification units (algorithm-shaped: they pin the classifier's
    // edges — one example per class plus the misses — and die with a
    // swap).
    // ------------------------------------------------------------------

    #[test]
    fn the_five_classes_and_the_miss() {
        assert_eq!(classify("todo", "todo"), Some(MatchQuality::Exact));
        assert_eq!(classify("todo list", "todo"), Some(MatchQuality::Prefix));
        assert_eq!(classify("my todo", "todo"), Some(MatchQuality::WordStart));
        // underscore counts as a boundary (D2's char-class generalization)
        assert_eq!(classify("foo_bar", "bar"), Some(MatchQuality::WordStart));
        assert_eq!(classify("autotodos", "todo"), Some(MatchQuality::Substring));
        assert_eq!(classify("t_d_o-x", "tdo"), Some(MatchQuality::Subsequence));
        assert_eq!(classify("cat", "dog"), None);
    }

    #[test]
    fn folding_is_unicode_lowercase_and_query_is_trimmed() {
        let state = state_from(vec![board(b(1), "Änderungen")], vec![], vec![]);
        assert_eq!(
            hit_targets(&find(&state, "  ÄNDER ")),
            vec![SearchTarget::Board(b(1))]
        );
    }

    #[test]
    fn a_prefix_is_promoted_over_the_word_start_it_contains() {
        // "todo" at index 0 is Prefix, not WordStart (class monotonicity
        // of the ladder).
        assert_eq!(classify("todo", "todo"), Some(MatchQuality::Exact));
        assert_eq!(classify("todo x", "todo"), Some(MatchQuality::Prefix));
    }

    #[test]
    fn out_of_scope_entities_never_hit() {
        // The scope predicates (D1): done AND archived tasks ARE hits
        // (tasks scope is `is_live()` = not tombstoned only); boards and
        // stacks must be live AND unarchived.
        let state = state_from(
            vec![
                board_flags(b(1), "todo", true, false),
                board_flags(b(2), "todo", false, true),
            ],
            vec![
                stack_flags(s(1), b(3), "todo", true, false),
                stack_flags(s(2), b(3), "todo", false, true),
            ],
            vec![
                {
                    let mut dead = task(t(1), "todo");
                    dead.deleted = true;
                    dead
                },
                {
                    let mut shelved = task(t(2), "todo");
                    shelved.archived = true;
                    shelved
                },
                {
                    let mut finished = task(t(3), "todo");
                    finished.done = Some(epoch());
                    finished
                },
            ],
        );
        assert_eq!(
            hit_targets(&find(&state, "todo")),
            vec![SearchTarget::Task(t(2)), SearchTarget::Task(t(3))]
        );
    }

    #[test]
    fn ranking_key_order_class_then_title_length_then_target() {
        // One worked example of the tuple order: class outranks length,
        // length outranks the (kind, id) tiebreak.
        let state = state_from(
            vec![board(b(1), "aa"), board(b(2), "aaa")],
            vec![stack(s(1), b(1), "xaax")],
            vec![task(t(1), "aa"), task(t(2), "aza")],
        );
        assert_eq!(
            hit_targets(&find(&state, "aa")),
            vec![
                SearchTarget::Task(t(1)),  // Exact, len 2
                SearchTarget::Board(b(1)), // Exact ("aa"), len 2
                SearchTarget::Board(b(2)), // Prefix ("aaa"), len 3
                SearchTarget::Stack(s(1)), // Substring ("xaax"), len 4
                SearchTarget::Task(t(2)),  // Subsequence ("aza"), len 3
            ]
        );
        assert_eq!(
            hit_qualities(&find(&state, "aa")),
            vec![
                MatchQuality::Exact,
                MatchQuality::Exact,
                MatchQuality::Prefix,
                MatchQuality::Substring,
                MatchQuality::Subsequence,
            ]
        );
    }

    // ------------------------------------------------------------------
    // Property suite. Each property is tagged contract-shaped (must
    // survive any find.rs swap) or algorithm-shaped (may move or die
    // with the current implementation).
    // ------------------------------------------------------------------

    fn title_len_of(state: &AppState, target: SearchTarget) -> usize {
        match target {
            SearchTarget::Task(id) => state.tasks[&id].title.chars().count(),
            SearchTarget::Stack(id) => state.stacks[&id].title.chars().count(),
            SearchTarget::Board(id) => state.boards[&id].title.chars().count(),
        }
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        /// contract-shaped: purity and canonicality — insertion order and
        /// call repetition never change the output.
        #[test]
        fn deterministic_across_insertion_order_and_calls(
            tasks in prop::collection::vec(task_strategy(), 0..12),
            query in string_strategy(),
        ) {
            let mut forward = AppState::default();
            for task in &tasks {
                forward.tasks.insert(task.id, task.clone());
            }
            let mut backward = AppState::default();
            for task in tasks.iter().rev() {
                backward.tasks.insert(task.id, task.clone());
            }
            let first = find(&forward, &query);
            prop_assert_eq!(first.clone(), find(&forward, &query));
            prop_assert_eq!(first, find(&backward, &query));
        }

        /// contract-shaped: ASCII case-flips never change the output.
        /// (Full Unicode folding is not an involution — `ß` ⇄ `SS` — so
        /// the flip is scoped to ASCII letters: an honest floor.)
        #[test]
        fn case_fold_invariant_under_ascii_case_flips(
            tasks in prop::collection::vec(task_strategy(), 0..12),
            query in "[a-zA-Z ]{1,16}",
        ) {
            let flipped: String = query
                .chars()
                .map(|c| if c.is_ascii_uppercase() {
                    c.to_ascii_lowercase()
                } else {
                    c.to_ascii_uppercase()
                })
                .collect();
            let mut state = AppState::default();
            for task in &tasks {
                state.tasks.insert(task.id, task.clone());
            }
            prop_assert_eq!(find(&state, &query), find(&state, &flipped));
        }

        /// contract-shaped: the empty-query floor (an empty box is not a
        /// search; floors on garbage input only — no ceilings).
        #[test]
        fn empty_query_yields_no_hits(
            tasks in prop::collection::vec(task_strategy(), 0..8),
            padding in prop::collection::vec(prop::sample::select(vec![' ', '\t']), 0..6),
        ) {
            let mut state = AppState::default();
            for task in &tasks {
                state.tasks.insert(task.id, task.clone());
            }
            let whitespace: String = padding.into_iter().collect();
            prop_assert_eq!(find(&state, ""), Vec::<SearchHit>::new());
            prop_assert_eq!(find(&state, &whitespace), Vec::<SearchHit>::new());
        }

        /// contract-shaped: planting an entity titled exactly the
        /// (folded) query guarantees an Exact hit inside the leading
        /// Exact block — every Exact hit precedes every non-Exact hit.
        /// (Other generated entities may fold to the query too, so the
        /// planted hit is first *within the Exact class* only per the
        /// `(title_len, target)` tiebreak, not necessarily first
        /// overall — the CI-caught input `query "b"` + a task titled
        /// "B" pinned the distinction.)
        #[test]
        fn exact_title_hit_ranks_first(
            tasks in prop::collection::vec(task_strategy(), 0..8),
            query in "[a-zA-Z0-9]{1,16}",
        ) {
            let needle = query.to_lowercase();
            let mut state = AppState::default();
            for task in &tasks {
                state.tasks.insert(task.id, task.clone());
            }
            state.tasks.insert(t(99), task(t(99), &needle));
            let hits = find(&state, &query);
            let planted = hits
                .iter()
                .position(|hit| hit.target == SearchTarget::Task(t(99)))
                .expect("the planted exact-title entity must hit");
            prop_assert_eq!(hits[planted].quality, MatchQuality::Exact);
            let exact_block =
                hits.iter().take_while(|hit| hit.quality == MatchQuality::Exact).count();
            prop_assert!(planted < exact_block);
            prop_assert!(hits[exact_block..]
                .iter()
                .all(|hit| hit.quality != MatchQuality::Exact));
        }

        /// contract-shaped: adding an entity never removes or reorders
        /// another entity's hit (growth monotonicity).
        #[test]
        fn growth_monotonicity(
            base in prop::collection::vec(task_strategy(), 0..8),
            extra in task_strategy(),
            query in string_strategy(),
        ) {
            let mut grown = AppState::default();
            for task in &base {
                grown.tasks.insert(task.id, task.clone());
            }
            let prior = find(&grown, &query);
            grown.tasks.insert(extra.id, extra);
            let after = find(&grown, &query);
            let restricted: Vec<SearchHit> = after
                .into_iter()
                .filter(|hit| prior.iter().any(|p| p.target == hit.target))
                .collect();
            prop_assert_eq!(prior, restricted);
        }

        /// contract-shaped: for each kind, filtering the full output by
        /// kind is an order-preserving partition (the `--type` lens and
        /// the palette's kind lenses ride on this).
        #[test]
        fn type_filter_preserves_order(
            tasks in prop::collection::vec(task_strategy(), 0..10),
            boards in prop::collection::vec(
                (any::<u128>(), string_strategy(), any::<bool>(), any::<bool>())
                    .prop_map(|(raw, title, archived, deleted)| {
                        board_flags(BoardId::from(uuid::Uuid::from_u128(raw)), &title, archived, deleted)
                    }),
                0..4,
            ),
            stacks in prop::collection::vec(
                (any::<u128>(), any::<u128>(), string_strategy(), any::<bool>(), any::<bool>())
                    .prop_map(|(raw, board_raw, title, archived, deleted)| {
                        stack_flags(
                            StackId::from(uuid::Uuid::from_u128(raw)),
                            BoardId::from(uuid::Uuid::from_u128(board_raw)),
                            &title,
                            archived,
                            deleted,
                        )
                    }),
                0..4,
            ),
            query in string_strategy(),
        ) {
            let mut state = AppState::default();
            for task in &tasks {
                state.tasks.insert(task.id, task.clone());
            }
            for board in boards {
                state.boards.insert(board.id, board);
            }
            for stack in stacks {
                state.stacks.insert(stack.id, stack);
            }
            let all = find(&state, &query);
            let task_disc = std::mem::discriminant(&SearchTarget::Task(t(0)));
            let stack_disc = std::mem::discriminant(&SearchTarget::Stack(s(0)));
            let board_disc = std::mem::discriminant(&SearchTarget::Board(b(0)));
            for disc in [task_disc, stack_disc, board_disc] {
                let mut remaining = all.iter();
                for hit in all.iter().filter(|h| std::mem::discriminant(&h.target) == disc) {
                    prop_assert!(
                        remaining.any(|h| h == hit),
                        "kind-filtered hits must stay a subsequence of the full output"
                    );
                }
            }
        }

        /// contract-shaped: the output is strictly ordered by the full
        /// ranking key and no target appears twice (the totality
        /// tiebreak: two entities sharing the full key cannot exist).
        #[test]
        fn output_is_strictly_ordered_and_duplicate_free(
            tasks in prop::collection::vec(task_strategy(), 0..12),
            query in string_strategy(),
        ) {
            let mut state = AppState::default();
            for task in &tasks {
                state.tasks.insert(task.id, task.clone());
            }
            let hits = find(&state, &query);
            let mut targets: Vec<SearchTarget> = hit_targets(&hits);
            targets.sort();
            let distinct = targets.len();
            targets.dedup();
            prop_assert_eq!(targets.len(), distinct);
            for pair in hits.windows(2) {
                let key = |hit: &SearchHit| {
                    (hit.quality, title_len_of(&state, hit.target), hit.target)
                };
                prop_assert!(key(&pair[0]) < key(&pair[1]), "keys must strictly increase");
            }
        }

        /// algorithm-shaped: whenever the folded title contains the
        /// folded query, a hit exists with quality at most Substring —
        /// the classes are refinements and the classifier returns the
        /// best attainable one.
        #[test]
        fn class_monotonicity(
            title in string_strategy(),
            query in string_strategy(),
        ) {
            let folded_title = title.to_lowercase();
            let folded_query = query.trim().to_lowercase();
            if !folded_query.is_empty() && folded_title.contains(&folded_query) {
                let state = state_from(vec![], vec![], vec![task(t(1), &title)]);
                let hits = find(&state, &query);
                prop_assert_eq!(hits.len(), 1);
                prop_assert!(hits[0].quality <= MatchQuality::Substring);
            }
        }
    }
}
