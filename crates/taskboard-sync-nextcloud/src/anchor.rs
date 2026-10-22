// SPDX-License-Identifier: MIT OR Apache-2.0
//! The anchor-card ceremony's pure decision face (D-S1): discovery scan,
//! deterministic pick, the action state machine, the creation target, and
//! the evidence builder. The actor keeps only cycle-level wiring; every
//! rule here is a pure function over the pulled snapshot and captured
//! card details.
//!
//! The scan is **exact-title** against [`ANCHOR_CARD_TITLE`] — no trim,
//! no casefold, deliberately unlike `ReconcileByTitle`: the convention
//! string is machine-owned. Anchor creation is lazy and actor-side
//! (never an engine command); this chunk's live cycle passes
//! `has_document_work = false`, so only [`AnchorAction::Use`] and
//! [`AnchorAction::None`] fire in production — `Adopt`/`Create` are
//! unit-pinned for chunk 4's flush.

use std::collections::HashMap;

use taskboard_domain::{
    ANCHOR_CARD_TITLE, APPDATA_FORMAT, AppDataEvidence, AppDataUnreadable, RemoteBoardId,
    RemoteCardRef, RemoteStack, RemoteStackId, RemoteTask,
};

use crate::model::Attachment;

/// The reserved stored filename of the sidecar document (stable forever —
/// no rename exists on the attachment surface).
pub(crate) const ANCHOR_ATTACHMENT_FILENAME: &str = "taskboard-appdata.json";

/// The sidecar document's MIME type (the flush uploads with it;
/// recorded here beside the filename it travels with).
pub(crate) const ANCHOR_ATTACHMENT_MIME: &str = "application/json";

/// One confirmed anchor carrier: a card whose detail read shows an
/// attachment with the reserved filename (live or tombstoned — both
/// count, D-S1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AnchorCandidate {
    /// The carrying card.
    pub card: RemoteCardRef,
    /// The document attachment's id.
    pub attachment_id: u64,
    /// The attachment's content stamp (epoch seconds) — the pick rule's
    /// primary key.
    pub last_modified: i64,
    /// The attachment's stored size — the change-detection pair's second
    /// half (no HTTP validators exist on the content route).
    pub filesize: i64,
    /// `true` when the only matching attachment is tombstoned: counts
    /// for confirmation, yields `Absent` evidence and no download.
    pub tombstoned: bool,
}

/// The discovery result over one board's snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct AnchorDiscovery {
    pub confirmed: Vec<AnchorCandidate>,
    /// Exact-title matches with no reserved-filename attachment in their
    /// (known) details — adoption candidates.
    pub unconfirmed: Vec<RemoteCardRef>,
}

/// Exact-title matches over the snapshot's card set (active + archived
/// already merged).
pub(crate) fn title_matches(
    snapshot: &taskboard_domain::RemoteBoardSnapshot,
) -> Vec<RemoteCardRef> {
    snapshot
        .tasks
        .iter()
        .filter(|task| task.title == ANCHOR_CARD_TITLE)
        .map(|task| task.id)
        .collect()
}

/// Classifies the exact-title matches: a candidate is *confirmed* when
/// its captured detail attachments carry the reserved filename (the
/// stored filename answers on the already-normalized
/// [`crate::model::ExtendedData::file_name`], D20), live **or**
/// tombstoned; otherwise *unconfirmed*.
pub(crate) fn scan_snapshot(
    snapshot: &taskboard_domain::RemoteBoardSnapshot,
    details: &HashMap<RemoteCardRef, Vec<Attachment>>,
) -> AnchorDiscovery {
    let mut discovery = AnchorDiscovery::default();
    for card_ref in title_matches(snapshot) {
        let Some(attachments) = details.get(&card_ref) else {
            discovery.unconfirmed.push(card_ref);
            continue;
        };
        let confirmed = attachments
            .iter()
            .filter(|attachment| stored_file_name(attachment) == ANCHOR_ATTACHMENT_FILENAME)
            .map(|attachment| AnchorCandidate {
                card: card_ref,
                attachment_id: attachment.id,
                last_modified: attachment.last_modified,
                filesize: attachment
                    .extended_data
                    .as_ref()
                    .map(|data| data.size)
                    .or_else(|| attachment.data.as_ref().map(|data| data.size))
                    .unwrap_or_default(),
                tombstoned: attachment.deleted_at != 0,
            })
            .next();
        match confirmed {
            Some(candidate) => discovery.confirmed.push(candidate),
            None => discovery.unconfirmed.push(card_ref),
        }
    }
    discovery
}

/// The stored filename of an attachment's metadata: the normalized
/// `extendedData.fileName`, else the `data` payload's, else empty.
fn stored_file_name(attachment: &Attachment) -> &str {
    attachment
        .extended_data
        .as_ref()
        .map(|data| data.file_name.as_str())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            attachment
                .data
                .as_ref()
                .map(|data| data.file_name.as_str())
                .unwrap_or_default()
        })
}

/// The ceremony's decision face: confirmed carrier ⇒ `Use` (the pick
/// rule; independent of document work — downloading is read-side); no
/// carrier + exactly one unconfirmed match + work ⇒ `Adopt` (upload onto
/// it — closes the interrupted-ceremony loop); no carrier + (zero or
/// ≥ 2 unconfirmed) + work ⇒ `Create` (never touch ambiguous user
/// cards); no work ⇒ `None` for the write arms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AnchorAction {
    Use(AnchorCandidate),
    Adopt(RemoteCardRef),
    Create,
    None,
}

/// The pick rule among confirmed carriers: newest attachment
/// `lastModified`, tie-break lowest remote card id — deterministic
/// across devices reading the same metadata. (Multiple
/// matching-filename attachments on one card cannot exist — the 409
/// duplicate rule — so per-card selection is unambiguous.)
#[must_use]
pub(crate) fn pick(discovery: &AnchorDiscovery) -> Option<&AnchorCandidate> {
    discovery.confirmed.iter().min_by_key(|candidate| {
        (
            std::cmp::Reverse(candidate.last_modified),
            candidate.card.card,
        )
    })
}

/// The action state machine (see [`AnchorAction`]).
#[must_use]
pub(crate) fn anchor_action(discovery: &AnchorDiscovery, has_document_work: bool) -> AnchorAction {
    if let Some(candidate) = pick(discovery) {
        return AnchorAction::Use(candidate.clone());
    }
    if !has_document_work {
        return AnchorAction::None;
    }
    match discovery.unconfirmed.as_slice() {
        [only] => AnchorAction::Adopt(*only),
        _ => AnchorAction::Create,
    }
}

/// The anchor card's creation position: the first live stack by `order`
/// (minimum), at last position (max card order in that stack + 1; 0 for
/// an empty stack). `None` when the board has no live stack — the flush
/// strands this cycle; the actor never auto-creates stacks. (Unit-pinned
/// here; consumed by the flush's Create ladder.)
#[must_use]
pub(crate) fn creation_target(
    stacks: &[RemoteStack],
    tasks: &[RemoteTask],
) -> Option<(RemoteStackId, i64)> {
    let stack = stacks
        .iter()
        .filter(|stack| stack.deleted_at.is_none() && !stack.archived)
        .min_by_key(|stack| (stack.order, stack.id.stack))?;
    let last_order = tasks
        .iter()
        .filter(|task| task.id.stack == stack.id.stack)
        .map(|task| task.order)
        .max()
        .unwrap_or(-1);
    Some((stack.id.stack, last_order + 1))
}

/// The evidence builder: a JSON slice decodes ⇒ envelope checks (strict
/// boundary: missing `format` / non-document JSON is `Decode`); a
/// too-new format or a foreign board key fold nothing, with the typed
/// reason.
#[must_use]
pub(crate) fn build_evidence(bytes: &[u8], board: RemoteBoardId) -> AppDataEvidence {
    let document: taskboard_domain::AppDataDocument = match serde_json::from_slice(bytes) {
        Ok(document) => document,
        Err(_) => return AppDataEvidence::Unreadable(AppDataUnreadable::Decode),
    };
    if document.format > APPDATA_FORMAT {
        return AppDataEvidence::Unreadable(AppDataUnreadable::FormatTooNew {
            format: document.format,
        });
    }
    if document.board != board {
        return AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard);
    }
    AppDataEvidence::Document(Box::new(document))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ExtendedData;
    use chrono::TimeZone;
    use taskboard_domain::RemoteCardId;
    use taskboard_domain::{RemoteBoardId, RemoteBoardSnapshot, RemoteStackId};

    fn card_ref(card: u64) -> RemoteCardRef {
        RemoteCardRef {
            board: RemoteBoardId(42),
            stack: RemoteStackId(7),
            card: RemoteCardId(card),
        }
    }

    fn task(id: u64, title: &str) -> RemoteTask {
        RemoteTask {
            id: card_ref(id),
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: RemoteStackId(7),
            order: 0,
            labels: std::collections::BTreeSet::new(),
            archived: false,
            last_modified: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
        }
    }

    fn snapshot_with(tasks: Vec<RemoteTask>) -> RemoteBoardSnapshot {
        RemoteBoardSnapshot {
            board: taskboard_domain::RemoteBoard {
                id: RemoteBoardId(42),
                title: "board".into(),
                color: "00ff00".into(),
                archived: false,
                deleted_at: None,
                last_modified: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            },
            stacks: Vec::new(),
            tasks,
            labels: Vec::new(),
        }
    }

    fn attachment(id: u64, file_name: &str, last_modified: i64, deleted_at: i64) -> Attachment {
        Attachment {
            id,
            card_id: 0,
            kind: "deck_file".into(),
            created_at: 0,
            created_by: String::new(),
            deleted_at,
            last_modified,
            data: None,
            extended_data: Some(ExtendedData {
                file_name: file_name.to_owned(),
                size: 25,
                mimetype: "application/json".into(),
                mtime: 0,
                info: None,
            }),
        }
    }

    fn details_for(
        card: u64,
        attachments: Vec<Attachment>,
    ) -> HashMap<RemoteCardRef, Vec<Attachment>> {
        HashMap::from([(card_ref(card), attachments)])
    }

    #[test]
    fn scan_matches_exact_titles_only() {
        let snapshot = snapshot_with(vec![
            task(5, ANCHOR_CARD_TITLE),
            task(6, "[taskboard] app-data "),  // trailing space
            task(7, "[Taskboard] app-data"),   // case
            task(8, "  [taskboard] app-data"), // leading whitespace
            task(9, "[taskboard] app-data"),
        ]);
        let matches = title_matches(&snapshot);
        assert_eq!(
            matches,
            vec![card_ref(5), card_ref(9)],
            "no normalization ever applies: exact string equality only"
        );
    }

    #[test]
    fn scan_confirms_on_the_reserved_filename_live_or_tombstoned() {
        let snapshot = snapshot_with(vec![task(5, ANCHOR_CARD_TITLE)]);
        let details = details_for(
            5,
            vec![
                attachment(11, ANCHOR_ATTACHMENT_FILENAME, 1_700_000_100, 0),
                attachment(12, "other.json", 1_700_000_200, 0),
            ],
        );
        let discovery = scan_snapshot(&snapshot, &details);
        assert_eq!(discovery.unconfirmed, Vec::new());
        let candidate = discovery.confirmed[0].clone();
        assert_eq!(candidate.attachment_id, 11);
        assert_eq!(candidate.last_modified, 1_700_000_100);
        assert_eq!(candidate.filesize, 25);
        assert!(!candidate.tombstoned);

        // A tombstoned reserved attachment still confirms — and marks
        // the tombstone.
        let tombstoned = details_for(
            5,
            vec![attachment(
                11,
                ANCHOR_ATTACHMENT_FILENAME,
                1_700_000_100,
                1_700_000_300,
            )],
        );
        let discovery = scan_snapshot(&snapshot, &tombstoned);
        assert!(discovery.confirmed[0].tombstoned);

        // No reserved filename: unconfirmed.
        let foreign = details_for(5, vec![attachment(12, "other.json", 5, 0)]);
        let discovery = scan_snapshot(&snapshot, &foreign);
        assert_eq!(discovery.unconfirmed, vec![card_ref(5)]);

        // No details at all (unbound card): unconfirmed.
        let discovery = scan_snapshot(&snapshot, &HashMap::new());
        assert_eq!(discovery.unconfirmed, vec![card_ref(5)]);
    }

    #[test]
    fn scan_falls_back_to_the_data_payloads_file_name() {
        // Some endpoints carry the stored filename only on `data`.
        let snapshot = snapshot_with(vec![task(5, ANCHOR_CARD_TITLE)]);
        let mut attachment = attachment(11, "", 50, 0);
        attachment.data = Some(ExtendedData {
            file_name: ANCHOR_ATTACHMENT_FILENAME.into(),
            ..ExtendedData::default()
        });
        let discovery = scan_snapshot(&snapshot, &details_for(5, vec![attachment]));
        assert_eq!(discovery.confirmed.len(), 1);
    }

    #[test]
    fn pick_prefers_the_newest_stamp_then_the_lowest_card_id() {
        let snapshot = snapshot_with(vec![
            task(5, ANCHOR_CARD_TITLE),
            task(9, ANCHOR_CARD_TITLE),
            task(7, ANCHOR_CARD_TITLE),
        ]);
        let mk = |card: u64, id: u64, stamp: i64| {
            details_for(
                card,
                vec![attachment(id, ANCHOR_ATTACHMENT_FILENAME, stamp, 0)],
            )
        };
        let mut details = HashMap::new();
        details.extend(mk(5, 1, 100));
        details.extend(mk(9, 2, 300));
        details.extend(mk(7, 3, 200));
        let discovery = scan_snapshot(&snapshot, &details);
        let winner = pick(&discovery).expect("three confirmed carriers");
        assert_eq!(winner.card.card, RemoteCardId(9), "newest stamp wins");

        // Tie on stamp: the lowest remote card id.
        let mut details = HashMap::new();
        details.extend(mk(5, 1, 100));
        details.extend(mk(9, 2, 100));
        details.extend(mk(7, 3, 100));
        let discovery = scan_snapshot(&snapshot, &details);
        assert_eq!(
            pick(&discovery)
                .expect("three confirmed carriers")
                .card
                .card,
            RemoteCardId(5)
        );
        assert_eq!(
            pick(&AnchorDiscovery::default()),
            None,
            "an empty discovery picks nothing"
        );
    }

    #[test]
    fn anchor_action_reads_the_state_machine() {
        // Confirmed carrier ⇒ Use, regardless of document work.
        let mut discovery = AnchorDiscovery::default();
        discovery.confirmed.push(AnchorCandidate {
            card: card_ref(5),
            attachment_id: 1,
            last_modified: 1,
            filesize: 1,
            tombstoned: false,
        });
        assert_eq!(
            anchor_action(&discovery, false),
            AnchorAction::Use(discovery.confirmed[0].clone())
        );
        assert_eq!(
            anchor_action(&discovery, true),
            AnchorAction::Use(discovery.confirmed[0].clone())
        );

        // No carrier, no work ⇒ None (this chunk's live cycle).
        let empty = AnchorDiscovery::default();
        assert_eq!(anchor_action(&empty, false), AnchorAction::None);

        // Exactly one unconfirmed + work ⇒ Adopt.
        let mut one = AnchorDiscovery::default();
        one.unconfirmed.push(card_ref(6));
        assert_eq!(anchor_action(&one, true), AnchorAction::Adopt(card_ref(6)));

        // Zero or ≥ 2 unconfirmed + work ⇒ Create (never ambiguous cards).
        assert_eq!(anchor_action(&empty, true), AnchorAction::Create);
        let mut two = AnchorDiscovery::default();
        two.unconfirmed.push(card_ref(6));
        two.unconfirmed.push(card_ref(8));
        assert_eq!(anchor_action(&two, true), AnchorAction::Create);
    }

    #[test]
    fn creation_target_picks_the_first_live_stack_at_last_position() {
        let stack = |id: u64, order: i64, deleted: bool, archived: bool| RemoteStack {
            id: taskboard_domain::RemoteStackRef {
                board: RemoteBoardId(42),
                stack: RemoteStackId(id),
            },
            title: "col".into(),
            order,
            archived,
            deleted_at: deleted.then_some(chrono::Utc.timestamp_opt(1, 0).unwrap()),
            last_modified: chrono::Utc.timestamp_opt(1, 0).unwrap(),
        };
        let task_in = |stack: u64, order: i64| {
            let mut t = task(1, "x");
            t.id.stack = RemoteStackId(stack);
            t.order = order;
            t
        };
        // Second stack has the lower order: it is first by `order`.
        let stacks = vec![stack(10, 1, false, false), stack(20, 0, false, false)];
        let tasks = vec![task_in(20, 3), task_in(20, 7), task_in(10, 1)];
        assert_eq!(
            creation_target(&stacks, &tasks),
            Some((RemoteStackId(20), 8))
        );
        // An empty first stack starts at 0.
        let tasks = vec![task_in(10, 5)];
        assert_eq!(
            creation_target(&stacks, &tasks),
            Some((RemoteStackId(20), 0))
        );
        // Deleted and archived stacks never host the anchor.
        let only_deleted = vec![stack(10, 0, true, false), stack(20, 1, false, true)];
        assert_eq!(creation_target(&only_deleted, &[]), None);
        // No stacks at all: the flush strands.
        assert_eq!(creation_target(&[], &[]), None);
    }

    #[test]
    fn build_evidence_enforces_the_envelope_checks() {
        // Undecodable body: Decode.
        assert_eq!(
            build_evidence(b"{not json", RemoteBoardId(42)),
            AppDataEvidence::Unreadable(AppDataUnreadable::Decode)
        );
        // Missing format: the strict envelope boundary, not leniency.
        assert_eq!(
            build_evidence(br#"{"board": 42, "sections": {}}"#, RemoteBoardId(42)),
            AppDataEvidence::Unreadable(AppDataUnreadable::Decode)
        );
        // Too-new format: folds nothing, the reason carries the version.
        assert_eq!(
            build_evidence(
                br#"{"format": 99, "board": 42, "sections": {}}"#,
                RemoteBoardId(42)
            ),
            AppDataEvidence::Unreadable(AppDataUnreadable::FormatTooNew { format: 99 })
        );
        // Foreign board: rode in on a moved card.
        assert_eq!(
            build_evidence(
                br#"{"format": 1, "board": 43, "sections": {}}"#,
                RemoteBoardId(42)
            ),
            AppDataEvidence::Unreadable(AppDataUnreadable::ForeignBoard)
        );
        // A matching document is Document evidence.
        let doc = AppDataEvidence::Document(Box::new(taskboard_domain::AppDataDocument {
            format: APPDATA_FORMAT,
            board: RemoteBoardId(42),
            sections: taskboard_domain::AppDataSections::default(),
            unknown: std::collections::BTreeMap::new(),
        }));
        assert_eq!(
            build_evidence(
                br#"{"format": 1, "board": 42, "sections": {}}"#,
                RemoteBoardId(42)
            ),
            doc
        );
    }
}
