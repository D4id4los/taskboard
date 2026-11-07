// SPDX-License-Identifier: AGPL-3.0-only

//! PDF export & print (#624 stage B): the `export --what pdf` leg's
//! engine. Compiled only behind the `pdf-export` feature of
//! `taskboard-app` — the default (kiosk) tree carries no PDF code, no
//! krilla, and no font bytes.
//!
//! Pipeline: [`print_day`] projects today's state (pure, over the
//! domain's [`taskboard_domain::days`] projection — no parallel
//! classification), [`layout`] turns a `PrintDay` into a fixed-geometry
//! display list (pure; wrapped and measured by our own code), and
//! [`render`] draws the display list with the embedded Noto Sans into
//! PDF bytes. [`deliver`] writes the file and opens the system viewer.
//!
//! Fixed curated layouts only (the R-4 ruling): A4, A5, A6 and the
//! foldable A4 sheet that folds into a four-page A6 booklet. No
//! user-editable templates, no silent printing (both fenced by ruling).

// Interim: the display-list types land before their consumers (the
// slices of plan §9); the allow dies with the verb wiring (step 6).
#![allow(dead_code)]

pub(crate) mod layout;
pub(crate) mod measure;
pub(crate) mod render;
