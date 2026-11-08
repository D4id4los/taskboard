// SPDX-License-Identifier: AGPL-3.0-only
//! Writes the "moved to files" marker into a Deck board's sidecar, the
//! only channel that reaches a device with no files tier (D-BF11).

use taskboard_domain::board_move::{MOVED_TO_FILES_KEY, MovedToFilesMarker};
use taskboard_domain::{
    ANCHOR_CARD_TITLE, APPDATA_FORMAT, AppDataDocument, AppDataEvidence, AppDataSections,
    RemoteBoardId,
};

use crate::anchor::{
    ANCHOR_ATTACHMENT_FILENAME, ANCHOR_ATTACHMENT_MIME, build_evidence, stored_file_name,
};
use crate::client::{AttachmentUpload, DeckClient, NewCard};
use crate::error::DeckError;
use crate::model::StackFilter;

/// Why the marker could not be written.
#[derive(Debug, thiserror::Error)]
pub enum MoveMarkerError {
    #[error("deck request failed")]
    Deck(#[from] DeckError),
    /// Rewriting a document this build cannot read would destroy it.
    #[error("the board's sidecar document is unreadable")]
    UnreadableDocument,
}

/// Merges the marker into the board's sidecar, creating the anchor card
/// when the board has none. A board with no live stack has nowhere to
/// carry it, and an already-deleted board has nobody left to tell.
///
/// # Errors
///
/// [`MoveMarkerError`]; idempotent, so a rerun repeats it safely.
///
/// # Panics
///
/// Never: the marker and document are plain serializable data.
pub async fn write_move_marker(
    client: &DeckClient,
    deck_board: u64,
    marker: MovedToFilesMarker,
) -> Result<(), MoveMarkerError> {
    let stacks = match client.stacks(deck_board, StackFilter::Active).await {
        Ok(stacks) => stacks,
        Err(DeckError::NotFound | DeckError::Forbidden) => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    let anchor = stacks
        .iter()
        .flat_map(|stack| stack.cards.iter().map(move |card| (stack.id, card)))
        .filter(|(_, card)| card.title == ANCHOR_CARD_TITLE)
        .min_by_key(|(_, card)| card.id)
        .map(|(stack, card)| (stack, card.id));
    let (stack, card) = if let Some(found) = anchor {
        found
    } else {
        let Some(first) = stacks.iter().min_by_key(|stack| (stack.order, stack.id)) else {
            return Ok(());
        };
        let new_card = NewCard {
            title: ANCHOR_CARD_TITLE.to_owned(),
            order: first
                .cards
                .iter()
                .map(|card| card.order)
                .max()
                .map(|top| top + 1),
            description: None,
            duedate: None,
            startdate: None,
            kind: None,
        };
        let created = client.create_card(deck_board, first.id, &new_card).await?;
        (first.id, created.id)
    };
    let existing = client
        .attachments(deck_board, stack, card)
        .await?
        .into_iter()
        .find(|item| item.deleted_at == 0 && stored_file_name(item) == ANCHOR_ATTACHMENT_FILENAME);
    let mut document = match &existing {
        Some(attachment) => {
            let bytes = client
                .download_attachment(deck_board, stack, card, attachment.id)
                .await?;
            match build_evidence(&bytes, RemoteBoardId::deck(deck_board)) {
                AppDataEvidence::Document(document) => *document,
                _ => return Err(MoveMarkerError::UnreadableDocument),
            }
        }
        None => AppDataDocument {
            format: APPDATA_FORMAT,
            board: RemoteBoardId::deck(deck_board),
            sections: AppDataSections::default(),
            unknown: std::collections::BTreeMap::new(),
        },
    };
    document.unknown.insert(
        MOVED_TO_FILES_KEY.to_owned(),
        serde_json::to_value(marker).expect("the marker serializes"),
    );
    let upload = AttachmentUpload {
        file_name: ANCHOR_ATTACHMENT_FILENAME.to_owned(),
        mime_type: ANCHOR_ATTACHMENT_MIME.to_owned(),
        content: serde_json::to_vec(&document).expect("the document serializes"),
    };
    match existing {
        Some(attachment) => {
            client
                .update_attachment_content(deck_board, card, attachment.id, &upload)
                .await?;
        }
        None => {
            client
                .upload_attachment(deck_board, stack, card, &upload)
                .await?;
        }
    }
    Ok(())
}
