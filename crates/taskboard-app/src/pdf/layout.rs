// SPDX-License-Identifier: AGPL-3.0-only

//! The print layouts' shared geometry vocabulary (plan §3.2/§4): the
//! point and page-size types, the fold sheet's panel placements, and
//! (in later steps) the display list. All coordinates are points
//! (1 pt = 1/72″), page origin at the top-left, y pointing down — the
//! convention the step-1 spike pinned for krilla, so the renderer
//! passes coordinates through unflipped.

/// A point in page coordinates (top-left origin, y down).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    /// Horizontal, rightward from the page's left edge.
    pub x: f32,
    /// Vertical, downward from the page's top edge.
    pub y: f32,
}

impl Pt {
    /// The point at `(x, y)`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A page size in points. Private fields and no runtime constructor:
/// the curated layout constants below are the only values, all
/// positive and finite.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    w_pt: f32,
    h_pt: f32,
}

impl PageSize {
    /// The width in points.
    #[must_use]
    pub const fn w_pt(self) -> f32 {
        self.w_pt
    }

    /// The height in points.
    #[must_use]
    pub const fn h_pt(self) -> f32 {
        self.h_pt
    }
}

/// A4 portrait (210 × 297 mm) — the fold sheet's physical page and the
/// A4 layout's page.
pub const A4: PageSize = PageSize {
    w_pt: 595.276,
    h_pt: 841.890,
};

/// One of the fold sheet's four panels, named by its booklet-page
/// role (reading order of the folded booklet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoldPanel {
    /// Booklet page 1 — the front cover. Top-left quadrant, upright.
    Cover,
    /// Booklet page 2. Top-right quadrant, upright.
    Two,
    /// Booklet page 3. Bottom-right quadrant, pre-rotated 180°.
    Three,
    /// Booklet page 4. Bottom-left quadrant, pre-rotated 180°.
    Four,
}

/// The panel→page placement map of report §3 (pinned cover-first): the
/// affine transform each panel's local coordinates undergo onto the A4
/// sheet. Panel content is laid out upright in panel-local coordinates
/// on `[0, w] × [0, h]`; the bottom row (booklet pages 3–4) is
/// pre-rotated 180° so it reads upright after the horizontal fold is
/// flipped over. Turned over, the sheet's bottom row reads mirrored
/// left-to-right (BR left of BL), so page 3 sits bottom-right and page
/// 4 bottom-left — the physical-fold amendment of the §3 table.
/// `Rot180 { tx, ty }` carries the translation (e, f) of the full
/// `[-1 0 0 -1 tx ty]` map.
#[must_use]
pub fn fold_panel_xf(panel: FoldPanel, sheet: PageSize) -> PanelXf {
    let w = sheet.w_pt() / 2.0;
    match panel {
        FoldPanel::Cover => PanelXf::Identity,
        FoldPanel::Two => PanelXf::Translated { dx: w, dy: 0.0 },
        FoldPanel::Three => PanelXf::Rot180 {
            tx: sheet.w_pt(),
            ty: sheet.h_pt(),
        },
        FoldPanel::Four => PanelXf::Rot180 {
            tx: w,
            ty: sheet.h_pt(),
        },
    }
}

/// The one affine placement a panel's display-list ops undergo — one
/// row of the pinned §4 table (`[a b c d e f]` in PDF order):
/// `Identity` = `[1 0 0 1 0 0]`, `Translated { dx, dy }` =
/// `[1 0 0 1 dx dy]`, `Rot180 { tx, ty }` = `[-1 0 0 -1 tx ty]`. The
/// renderer pushes each rotated (and translated) panel's ops with
/// krilla's `push_transform`/`pop` — it never rotates glyphs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelXf {
    /// Placement as laid out (the cover panel).
    Identity,
    /// A pure translation (the top row's right panel).
    Translated {
        /// The x shift.
        dx: f32,
        /// The y shift.
        dy: f32,
    },
    /// A 180° rotation about the panel origin, then a translation:
    /// sheet = (tx − x, ty − y). Exactly krilla's
    /// `Transform::from_row(-1, 0, 0, -1, tx, ty)` — the bottom row's
    /// pre-rotation so it reads upright after the fold is flipped.
    Rot180 {
        /// The x of the sheet point local (0, 0) lands on.
        tx: f32,
        /// The y of the sheet point local (0, 0) lands on.
        ty: f32,
    },
}

impl PanelXf {
    /// Maps a panel-local point onto the sheet (the pure twin of the
    /// renderer's pushed transform — every layout property test walks
    /// this).
    #[cfg(test)]
    #[must_use]
    pub fn apply(&self, p: Pt) -> Pt {
        match self {
            Self::Identity => p,
            Self::Translated { dx, dy } => Pt::new(p.x + dx, p.y + dy),
            Self::Rot180 { tx, ty } => Pt::new(tx - p.x, ty - p.y),
        }
    }
}

// ---- The display list and the print-day projection (plan §3) -----------

use crate::pdf::measure::{Measure, wrap};

/// A drawn stroke weight (the embedded face's two weights).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    /// Noto Sans Regular — the body.
    Regular,
    /// Noto Sans Bold — headings and the header line.
    Bold,
}

/// One drawing primitive, panel-local coordinates plus the placement
/// that maps them onto the sheet (identity for single-sided pages).
#[derive(Debug, Clone, PartialEq)]
pub enum DrawOp {
    /// A text run; `at` is the baseline's left end.
    Text {
        /// Baseline-left point.
        at: Pt,
        /// Font size in points.
        size_pt: f32,
        /// The stroke weight.
        weight: Weight,
        /// The run (single line — wrapping already happened).
        text: String,
        /// The panel placement.
        panel: PanelXf,
    },
    /// A straight rule from `from` to `to`.
    Rule {
        /// Start point.
        from: Pt,
        /// End point.
        to: Pt,
        /// Stroke width in points.
        width_pt: f32,
        /// The panel placement.
        panel: PanelXf,
    },
    /// A checkbox square; a ticked one gets the tick stroke (drawn as
    /// paths by the renderer — never font glyphs).
    Checkbox {
        /// The square's top-left point.
        at: Pt,
        /// Edge length in points.
        side_pt: f32,
        /// Whether the tick is drawn.
        ticked: bool,
        /// The panel placement.
        panel: PanelXf,
    },
}

/// One physical page of the printout.
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    /// The physical page size.
    pub size: PageSize,
    /// The display-list ops, in draw order.
    pub ops: Vec<DrawOp>,
}

/// The today-projection section a print section renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Live ∧ open ∧ due before now.
    Overdue,
    /// Live ∧ open ∧ scheduled in today's window.
    Scheduled,
    /// Live ∧ open ∧ due later today.
    DueToday,
    /// Live ∧ completed in today's window.
    Done,
}

impl SectionKind {
    /// The section's heading (the today view's vocabulary).
    #[must_use]
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Overdue => "Overdue",
            Self::Scheduled => "Scheduled",
            Self::DueToday => "Due today",
            Self::Done => "Done",
        }
    }
}

/// One printable line-item: a checkbox, an optional time column, a
/// wrapped title, an optional right-aligned effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintItem {
    /// The task title (wrapped by the layout).
    pub title: String,
    /// Whether the checkbox is ticked (Done items only).
    pub done: bool,
    /// The absolute time column (`HH:MM`, optionally with a `dd.mm.`
    /// date prefix on overdue items) — never a relative phrase, which
    /// would go stale once printed (plan §3.1).
    pub time: Option<String>,
    /// The effort phrase, right-aligned on the item's first line.
    pub effort: Option<String>,
}

/// One section of the print day: heading plus items. Empty sections
/// are omitted at projection time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintSection {
    /// Which section this is.
    pub kind: SectionKind,
    /// The items, in the today projection's canonical order.
    pub items: Vec<PrintItem>,
}

/// The whole printable day (plan §3.1): the projected sections plus
/// the header date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintDay {
    /// The window-date of `now` (`calendar.today(now)`).
    pub date: chrono::NaiveDate,
    /// The sections in display order: Overdue, Scheduled, `DueToday`,
    /// Done — empty ones omitted.
    pub sections: Vec<PrintSection>,
}

/// The overdue item's absolute time column: `HH:MM`, prefixed with the
/// due date as `dd.mm.` when the due date is not today (an overdue
/// sheet that says "09:00" without the day would lie). Pure; unit-
/// pinned.
#[must_use]
pub fn overdue_time_phrase(
    due: chrono::DateTime<chrono::Utc>,
    today: chrono::NaiveDate,
    calendar: taskboard_domain::DayCalendar,
) -> String {
    let time = taskboard_ui_slint::presentation::plan_box_time_phrase(Some(due), calendar);
    if calendar.calendar_date(due) == today {
        time
    } else {
        format!(
            "{} {time}",
            due.with_timezone(&calendar.offset).format("%d.%m.")
        )
    }
}

/// The sheet header: ISO date plus short weekday (`2026-11-07 Sat`).
/// `review_day_phrase` is plain ISO (no weekday), so the weekday is
/// appended here — the offset-local reading, same as the review phrase.
#[must_use]
pub fn header_phrase(date: chrono::NaiveDate) -> String {
    date.format("%Y-%m-%d %a").to_string()
}

/// Projects the printable day from the state engine's snapshot: the
/// domain's own today projection ([`taskboard_domain::days`]) read by
/// id — no parallel classification happens here. `now` travels as data
/// (the clock rule): the caller captures it once.
#[must_use]
pub fn print_day(
    state: &taskboard_domain::AppState,
    now: chrono::DateTime<chrono::Utc>,
    calendar: taskboard_domain::DayCalendar,
) -> PrintDay {
    use taskboard_ui_slint::presentation::{effort_phrase, plan_box_time_phrase};
    let window = taskboard_domain::days::day_window(now, calendar.day_start_time, calendar.offset);
    let sections = taskboard_domain::days::today_sections(&state.tasks, now, window);
    let today = calendar.today(now);
    let item = |id: taskboard_domain::TaskId, done: bool| {
        let task = &state.tasks[&id];
        PrintItem {
            title: task.title.clone(),
            done,
            time: match (done, task.duedate, task.scheduled) {
                (true, _, _) => task.done.map(|t| plan_box_time_phrase(Some(t), calendar)),
                (false, Some(due), _) => Some(overdue_time_phrase(due, today, calendar)),
                (false, None, Some(scheduled)) => {
                    Some(plan_box_time_phrase(Some(scheduled), calendar))
                }
                (false, None, None) => None,
            },
            effort: task
                .effort
                .map(|e| effort_phrase(Some(e)))
                .filter(|phrase| !phrase.is_empty()),
        }
    };
    let section = |kind, ids: &[taskboard_domain::TaskId]| PrintSection {
        kind,
        items: ids
            .iter()
            .map(|id| {
                let done = matches!(kind, SectionKind::Done);
                item(*id, done)
            })
            .collect(),
    };
    let mut day = PrintDay {
        date: today,
        sections: vec![
            section(SectionKind::Overdue, &sections.overdue),
            section(SectionKind::Scheduled, &sections.scheduled),
            section(SectionKind::DueToday, &sections.due_today),
            section(SectionKind::Done, &sections.done_today),
        ],
    };
    day.sections.retain(|s| !s.items.is_empty());
    day
}

/// The display time column of an item, given its section: the item's
/// own `time` (already absolute — see [`PrintItem`]).
const TIME_COLUMN_CHARS: usize = 8;

/// A curated print layout (the R-4 ruling: fixed, four).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintLayout {
    /// A4 portrait, two columns.
    A4,
    /// A5 portrait, one column.
    A5,
    /// A6 portrait, one column.
    A6,
    /// The foldable sheet: one A4 page folding into a four-page A6
    /// booklet (cover-first, plan §4).
    FoldA6x4,
}

/// One layout's fixed geometry (the goldens freeze the values).
#[derive(Debug, Clone, Copy)]
pub struct LayoutSpec {
    /// The physical page.
    pub page: PageSize,
    /// The page margin (per panel on the fold sheet), in points.
    pub margin_pt: f32,
    /// Body type size.
    pub body_pt: f32,
    /// Heading size.
    pub heading_pt: f32,
    /// Content columns per sheet.
    pub columns: usize,
    /// Whether this is the fold imposition (panel-local coordinates,
    /// header only on the cover panel, `+N more` overflow).
    pub fold: bool,
}

impl PrintLayout {
    /// The layout's fixed geometry (plan §3.2's starting values; the
    /// goldens freeze them).
    #[must_use]
    pub const fn spec(self) -> LayoutSpec {
        const MM: f32 = 72.0 / 25.4;
        match self {
            Self::A4 => LayoutSpec {
                page: A4,
                margin_pt: 10.0 * MM,
                body_pt: 10.0,
                heading_pt: 13.0,
                columns: 2,
                fold: false,
            },
            Self::A5 => LayoutSpec {
                page: PageSize {
                    w_pt: 148.0 * MM,
                    h_pt: 210.0 * MM,
                },
                margin_pt: 8.0 * MM,
                body_pt: 9.0,
                heading_pt: 11.0,
                columns: 1,
                fold: false,
            },
            Self::A6 => LayoutSpec {
                page: PageSize {
                    w_pt: 105.0 * MM,
                    h_pt: 148.0 * MM,
                },
                margin_pt: 6.0 * MM,
                body_pt: 8.0,
                heading_pt: 10.0,
                columns: 1,
                fold: false,
            },
            Self::FoldA6x4 => LayoutSpec {
                page: A4,
                margin_pt: 5.0 * MM,
                body_pt: 8.0,
                heading_pt: 10.0,
                columns: 1,
                fold: true,
            },
        }
    }
}

/// A content box: one column of one page (or one panel of the fold
/// sheet). Coordinates are sheet coordinates for single-sided pages,
/// panel-local for the fold (the placement rides the ops).
#[derive(Debug, Clone, Copy)]
struct Box {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    panel: PanelXf,
    /// Which physical page this box sits on (fold: always 0).
    page: usize,
}

/// The content boxes of a layout, in flow order.
fn content_boxes(spec: &LayoutSpec) -> Vec<Box> {
    if spec.fold {
        let w = spec.page.w_pt() / 2.0;
        let h = spec.page.h_pt() / 2.0;
        let m = spec.margin_pt;
        let placements = [
            FoldPanel::Cover,
            FoldPanel::Two,
            FoldPanel::Three,
            FoldPanel::Four,
        ];
        placements
            .into_iter()
            .map(|panel| {
                let xf = fold_panel_xf(panel, spec.page);
                Box {
                    x: m,
                    y: m,
                    w: w - 2.0 * m,
                    h: h - 2.0 * m,
                    panel: xf,
                    page: 0,
                }
            })
            .collect()
    } else {
        let cols = spec.columns.max(1);
        let m = spec.margin_pt;
        let gutter = m;
        #[allow(clippy::cast_precision_loss)] // 1–2 columns
        let w = (spec.page.w_pt() - 2.0 * m - (cols - 1) as f32 * gutter) / cols as f32;
        let h = spec.page.h_pt() - 2.0 * m;
        // The single-sided layouts paginate without bound; capacity
        // grows on demand in `layout_day`, so a nominal two-page box
        // list is enough as a template.
        (0..2)
            .flat_map(|page| {
                (0..cols).map(move |col| {
                    #[allow(clippy::cast_precision_loss)] // 1–2 columns
                    let x = m + (w + gutter) * col as f32;
                    Box {
                        x,
                        y: m,
                        w,
                        h,
                        panel: PanelXf::Identity,
                        page,
                    }
                })
            })
            .collect()
    }
}

/// One content line the flow engine places: a section heading or a
/// print item.
enum Line {
    Heading(&'static str),
    Item(PrintItem),
}

/// The wrapped text lines of an item (the flow slot consumes all of
/// them so continuation lines never collide with the next item).
fn item_lines(spec: &LayoutSpec, item: &PrintItem, m: &impl Measure, col_w: f32) -> Vec<String> {
    let checkbox_side = spec.body_pt * 0.8;
    let indent = checkbox_side + 2.0;
    let time_w = m.advance_pt(&"0".repeat(TIME_COLUMN_CHARS), spec.body_pt) + 4.0;
    let effort_w = item
        .effort
        .as_ref()
        .map_or(0.0, |e| m.advance_pt(e, spec.body_pt) + 6.0);
    let title_w = (col_w - indent - time_w - effort_w).max(spec.body_pt * 4.0);
    wrap(&item.title, title_w, spec.body_pt, m)
}

/// Opens a content box: draws the sheet header on a new physical page
/// (single-sided: every page; fold: only the cover panel) and returns
/// the flow start position.
#[allow(clippy::too_many_lines)] // one explicit branch per case, kept flat for review
fn open_content_box(
    ops: &mut Vec<DrawOp>,
    spec: &LayoutSpec,
    header: &str,
    current_page: Option<usize>,
    box_: &Box,
    fold: bool,
) -> (Option<usize>, f32) {
    if fold {
        if current_page.is_none() {
            ops.push(DrawOp::Text {
                at: Pt::new(box_.x, box_.y + spec.heading_pt),
                size_pt: spec.heading_pt,
                weight: Weight::Bold,
                text: header.to_owned(),
                panel: box_.panel,
            });
            return (Some(0), box_.y + spec.heading_pt * 1.5);
        }
        return (current_page, box_.y);
    }
    if current_page.is_none_or(|page| page != box_.page) {
        ops.push(DrawOp::Text {
            at: Pt::new(box_.x, box_.y + spec.heading_pt),
            size_pt: spec.heading_pt,
            weight: Weight::Bold,
            text: header.to_owned(),
            panel: box_.panel,
        });
        (Some(box_.page), box_.y + spec.heading_pt * 1.5)
    } else {
        (current_page, box_.y)
    }
}

/// Draws one item line group (checkbox, time, right-aligned effort,
/// wrapped title) at the current flow position.
fn draw_item(
    ops: &mut Vec<DrawOp>,
    spec: &LayoutSpec,
    item: &PrintItem,
    box_: &Box,
    y: f32,
    m: &impl Measure,
) {
    let checkbox_side = spec.body_pt * 0.8;
    let indent = checkbox_side + 2.0;
    let time_w = m.advance_pt(&"0".repeat(TIME_COLUMN_CHARS), spec.body_pt) + 4.0;
    let first_y = y + spec.body_pt;
    ops.push(DrawOp::Checkbox {
        at: Pt::new(box_.x, first_y - checkbox_side),
        side_pt: checkbox_side,
        ticked: item.done,
        panel: box_.panel,
    });
    if let Some(time) = &item.time {
        ops.push(DrawOp::Text {
            at: Pt::new(box_.x + indent, first_y),
            size_pt: spec.body_pt,
            weight: Weight::Regular,
            text: time.clone(),
            panel: box_.panel,
        });
    }
    if let Some(effort) = &item.effort {
        let w = m.advance_pt(effort, spec.body_pt);
        ops.push(DrawOp::Text {
            at: Pt::new(box_.x + box_.w - w, first_y),
            size_pt: spec.body_pt,
            weight: Weight::Regular,
            text: effort.clone(),
            panel: box_.panel,
        });
    }
    let title_x = box_.x + indent + time_w;
    for (n, tl) in item_lines(spec, item, m, box_.w).iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // a small line index
        let ny = u32::try_from(n).unwrap_or(u32::MAX) as f32;
        ops.push(DrawOp::Text {
            at: Pt::new(title_x, first_y + ny * spec.body_pt * 1.7),
            size_pt: spec.body_pt,
            weight: Weight::Regular,
            text: tl.clone(),
            panel: box_.panel,
        });
    }
}

/// Lays a [`PrintDay`] out into physical sheets (plan §3.2): fixed
/// geometry, greedy flow, the header repeated on every continued
/// single-sided sheet, the fold sheet capped at its four panels with a
/// `+N more` marker for whatever did not fit, and the Notes ruled area
/// filling what remains of the last box. Pure; deterministic; the
/// goldens freeze the geometry.
#[must_use]
#[allow(clippy::too_many_lines)] // the greedy flow is one flat loop, kept flat for review
pub fn layout_day(day: &PrintDay, layout: PrintLayout, m: &impl Measure) -> Vec<Sheet> {
    let spec = layout.spec();
    let mut blocks: Vec<Line> = Vec::new();
    for section in &day.sections {
        blocks.push(Line::Heading(section.kind.heading()));
        for item in &section.items {
            blocks.push(Line::Item(item.clone()));
        }
    }
    // The Notes area is always present: its heading flows with the
    // sections, the ruled lines fill the rest of the last box.
    blocks.push(Line::Heading("Notes"));

    // The box sequence: the fold's four panels, then exhausted (the
    // booklet is one sheet by construction); the single-sided layouts
    // paginate on demand, header included.
    let mut next_box = {
        let template = content_boxes(&spec);
        let mut fold_left: Vec<Box> = if spec.fold {
            // Popped from the back: reverse so the flow runs p1..p4.
            let mut boxes = template.clone();
            boxes.reverse();
            boxes
        } else {
            Vec::new()
        };
        let mut page = 0usize;
        let mut col = 0usize;
        move || {
            if spec.fold {
                return fold_left.pop();
            }
            let cols = spec.columns.max(1);
            if col >= cols {
                col = 0;
                page += 1;
            }
            let mut box_ = template[col];
            box_.page = page;
            col += 1;
            Some(box_)
        }
    };

    let mut sheets: Vec<Sheet> = Vec::new();
    let mut ops: Vec<DrawOp> = Vec::new();
    let mut current_page: Option<usize> = None;
    let mut open_box: Option<Box> = None;
    let mut last_box: Option<Box> = None;
    let mut y = 0.0f32;

    let mut i = 0usize;
    let mut overflow: Option<usize> = None;
    while i < blocks.len() {
        let Some(open) = open_box else {
            let Some(box_) = next_box() else {
                overflow = Some(blocks.len() - i);
                break;
            };
            // The fold's cover panel opens with the header; the other
            // panels are headerless continuations. A new physical
            // single-sided sheet flushes the old one (if it drew
            // anything — never an empty page 0) and repeats the header.
            if !spec.fold && !ops.is_empty() && current_page != Some(box_.page) {
                sheets.push(Sheet {
                    size: spec.page,
                    ops: std::mem::take(&mut ops),
                });
            }
            let (page, start_y) = open_content_box(
                &mut ops,
                &spec,
                &header_phrase(day.date),
                current_page,
                &box_,
                spec.fold,
            );
            current_page = page;
            y = start_y;
            open_box = Some(box_);
            last_box = Some(box_);
            continue;
        };
        let box_ = &open;
        let height = match &blocks[i] {
            Line::Heading(_) => spec.heading_pt * 1.5,
            Line::Item(item) => {
                #[allow(clippy::cast_precision_loss)] // a wrapped-line count, far below f32 reach
                let lines = item_lines(&spec, item, m, box_.w).len() as f32;
                lines * spec.body_pt * 1.7
            }
        };
        if y + height > box_.y + box_.h {
            open_box = None; // advance to the next box
            continue;
        }
        match &blocks[i] {
            Line::Heading(text) => {
                ops.push(DrawOp::Text {
                    at: Pt::new(box_.x, y + spec.heading_pt),
                    size_pt: spec.heading_pt,
                    weight: Weight::Bold,
                    text: (*text).to_owned(),
                    panel: box_.panel,
                });
            }
            Line::Item(item) => draw_item(&mut ops, &spec, item, box_, y, m),
        }
        y += height;
        i += 1;
    }

    // The fold overflow marker: the omitted count rides the last
    // (fourth) panel as its final line.
    if let Some(n) = overflow {
        if let Some(box_) = open_box.or(last_box) {
            let mark_y = (y + spec.body_pt * 1.7).min(box_.y + box_.h);
            ops.push(DrawOp::Text {
                at: Pt::new(box_.x, mark_y),
                size_pt: spec.body_pt,
                weight: Weight::Regular,
                text: format!("+{n} more"),
                panel: box_.panel,
            });
        }
    } else if let Some(box_) = open_box {
        // Notes: ruled lines fill the rest of the box the flow ended
        // in (plan §3.2, the prior art's non-routine space).
        let slot = spec.body_pt * 1.7;
        let mut ny = y + slot;
        while ny <= box_.y + box_.h {
            ops.push(DrawOp::Rule {
                from: Pt::new(box_.x, ny),
                to: Pt::new(box_.x + box_.w, ny),
                width_pt: 0.5,
                panel: box_.panel,
            });
            ny += slot;
        }
    }
    if !ops.is_empty() || sheets.is_empty() {
        sheets.push(Sheet {
            size: spec.page,
            ops,
        });
    }
    sheets
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const TOL: f32 = 0.001;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < TOL
    }

    /// Each panel's transform maps its local corners exactly onto its
    /// quadrant (the plan §4 check for p3 spelled out: local (0,0) →
    /// the quadrant's far corner, local (w,h) → its near corner).
    #[test]
    fn panel_xfs_map_the_corners_onto_their_quadrants() {
        let sheet = A4;
        let w = sheet.w_pt() / 2.0;
        let h = sheet.h_pt() / 2.0;

        let cases = [
            (FoldPanel::Cover, Pt::new(0.0, 0.0), Pt::new(0.0, 0.0)),
            (FoldPanel::Cover, Pt::new(w, h), Pt::new(w, h)),
            (FoldPanel::Two, Pt::new(0.0, 0.0), Pt::new(w, 0.0)),
            (FoldPanel::Two, Pt::new(w, h), Pt::new(sheet.w_pt(), h)),
            (
                FoldPanel::Three,
                Pt::new(0.0, 0.0),
                Pt::new(sheet.w_pt(), sheet.h_pt()),
            ),
            (FoldPanel::Three, Pt::new(w, h), Pt::new(w, h)),
            (FoldPanel::Four, Pt::new(0.0, 0.0), Pt::new(w, sheet.h_pt())),
            (FoldPanel::Four, Pt::new(w, h), Pt::new(0.0, h)),
        ];
        for (panel, local, expected) in cases {
            let got = fold_panel_xf(panel, sheet).apply(local);
            assert!(
                close(got.x, expected.x) && close(got.y, expected.y),
                "{panel:?} local {local:?} → {got:?}, expected {expected:?}"
            );
        }
    }

    /// The double-application law: rotating 180° twice about the same
    /// anchor is the identity (the inverse of `Rot180 { tx, ty }` is
    /// itself).
    #[test]
    fn rot180_applied_twice_is_the_identity() {
        let xf = PanelXf::Rot180 {
            tx: 297.638,
            ty: 841.890,
        };
        let p = Pt::new(123.4, 56.7);
        let twice = xf.apply(xf.apply(p));
        assert!(close(twice.x, p.x) && close(twice.y, p.y));
    }

    proptest! {
        /// Every panel-local point lands inside its quadrant, and the
        /// four quadrant images tile the sheet: no gap (each quadrant's
        /// image fills it — corner pairing above) and no overlap (the
        /// quadrants are disjoint by construction, and each point stays
        /// in its own).
        #[test]
        fn every_mapped_point_stays_inside_its_quadrant(
            x in 0.0..297.638f32,
            y in 0.0..420.945f32,
        ) {
            let sheet = A4;
            let w = sheet.w_pt() / 2.0;
            let h = sheet.h_pt() / 2.0;
            let local = Pt::new(x, y);
            let expected = [
                (FoldPanel::Cover, 0.0, w, 0.0, h),
                (FoldPanel::Two, w, sheet.w_pt(), 0.0, h),
                (FoldPanel::Three, w, sheet.w_pt(), h, sheet.h_pt()),
                (FoldPanel::Four, 0.0, w, h, sheet.h_pt()),
            ];
            for (panel, x0, x1, y0, y1) in expected {
                let got = fold_panel_xf(panel, sheet).apply(local);
                prop_assert!(got.x >= x0 - TOL && got.x <= x1 + TOL);
                prop_assert!(got.y >= y0 - TOL && got.y <= y1 + TOL);
            }
        }
    }

    // ---- Step 4: layouts, goldens, overflow, print_day ---------------------

    use crate::pdf::measure::FixedWidth;
    use insta::assert_snapshot;
    use std::collections::BTreeMap;
    use taskboard_domain::{AppState, DayCalendar};

    fn fixture_day() -> PrintDay {
        PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections: vec![
                PrintSection {
                    kind: SectionKind::Overdue,
                    items: vec![PrintItem {
                        title: "Müßiggang „Prüfung“ – Größe: ein ausgesprochen langer ".to_owned()
                            + "Titel, der in jeder Spalte umbricht",
                        done: false,
                        time: Some("07.11. 09:00".to_owned()),
                        effort: Some("2h".to_owned()),
                    }],
                },
                PrintSection {
                    kind: SectionKind::Scheduled,
                    items: vec![
                        PrintItem {
                            title: "Board review".to_owned(),
                            done: false,
                            time: Some("09:30".to_owned()),
                            effort: None,
                        },
                        PrintItem {
                            title: "Zweitinvoice prüfen".to_owned(),
                            done: false,
                            time: Some("11:00".to_owned()),
                            effort: Some("30m".to_owned()),
                        },
                    ],
                },
                PrintSection {
                    kind: SectionKind::DueToday,
                    items: vec![PrintItem {
                        title: "Steuer abgeben".to_owned(),
                        done: false,
                        time: Some("18:00".to_owned()),
                        effort: None,
                    }],
                },
                PrintSection {
                    kind: SectionKind::Done,
                    items: vec![PrintItem {
                        title: "Morgenroutine".to_owned(),
                        done: true,
                        time: Some("07:00".to_owned()),
                        effort: None,
                    }],
                },
            ],
        }
    }

    /// One line per op, coordinates rounded to 0.01 pt — the stable
    /// text dump the goldens freeze (goldens as data; regeneration is
    /// the explicit `INSTA_UPDATE=always` run recorded in the commit).
    fn sheet_dump(sheets: &[Sheet]) -> String {
        let mut out = String::new();
        for (page, sheet) in sheets.iter().enumerate() {
            let header = format!(
                "== page {page} {}x{:.2} ==\n",
                sheet.size.w_pt(),
                sheet.size.h_pt()
            );
            out.push_str(&header);
            for op in &sheet.ops {
                match op {
                    DrawOp::Text {
                        at,
                        size_pt,
                        weight,
                        text,
                        panel,
                    } => {
                        let line = format!(
                            "text {:.2},{:.2} {:.1}pt {:?} {:?} | {text}\n",
                            at.x,
                            at.y,
                            size_pt,
                            weight,
                            panel_kind(panel)
                        );
                        out.push_str(&line);
                    }
                    DrawOp::Rule {
                        from,
                        to,
                        width_pt,
                        panel,
                    } => {
                        let line = format!(
                            "rule {:.2},{:.2}->{:.2},{:.2} w{:.1} {:?}\n",
                            from.x,
                            from.y,
                            to.x,
                            to.y,
                            width_pt,
                            panel_kind(panel)
                        );
                        out.push_str(&line);
                    }
                    DrawOp::Checkbox {
                        at,
                        side_pt,
                        ticked,
                        panel,
                    } => {
                        let line = format!(
                            "check {:.2},{:.2} s{:.1} ticked={ticked} {:?}\n",
                            at.x,
                            at.y,
                            side_pt,
                            panel_kind(panel)
                        );
                        out.push_str(&line);
                    }
                }
            }
        }
        out
    }

    /// The placement with its translation: the golden pins which
    /// quadrant each panel lands on, not just the transform's shape.
    fn panel_kind(panel: &PanelXf) -> String {
        match panel {
            PanelXf::Identity => "id".to_owned(),
            PanelXf::Translated { dx, dy } => format!("tr@{dx:.2},{dy:.2}"),
            PanelXf::Rot180 { tx, ty } => format!("rot@{tx:.2},{ty:.2}"),
        }
    }

    #[test]
    fn golden_a4_layout_two_columns() {
        let sheets = layout_day(&fixture_day(), PrintLayout::A4, &FixedWidth(0.6));
        assert_snapshot!(sheet_dump(&sheets));
    }

    #[test]
    fn golden_a5_layout() {
        let sheets = layout_day(&fixture_day(), PrintLayout::A5, &FixedWidth(0.6));
        assert_snapshot!(sheet_dump(&sheets));
    }

    #[test]
    fn golden_a6_layout() {
        let sheets = layout_day(&fixture_day(), PrintLayout::A6, &FixedWidth(0.6));
        assert_snapshot!(sheet_dump(&sheets));
    }

    #[test]
    fn golden_fold_layout_four_panels_one_sheet() {
        // The fixture plus 120 generated items: enough to flow through
        // all four panels, so the golden pins the whole fold table.
        let mut day = fixture_day();
        let scheduled = day
            .sections
            .iter_mut()
            .find(|s| s.kind == SectionKind::Scheduled)
            .expect("the fixture has a scheduled section");
        for n in 0..120 {
            scheduled.items.push(PrintItem {
                title: format!("Aufgabe {n:03}"),
                done: false,
                time: None,
                effort: None,
            });
        }
        let sheets = layout_day(&day, PrintLayout::FoldA6x4, &FixedWidth(0.6));
        assert_eq!(sheets.len(), 1);
        assert_snapshot!(sheet_dump(&sheets));
    }

    #[test]
    fn golden_empty_day_fold_is_header_and_notes_only() {
        let day = PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections: Vec::new(),
        };
        let sheets = layout_day(&day, PrintLayout::FoldA6x4, &FixedWidth(0.6));
        assert_eq!(sheets.len(), 1);
        assert_snapshot!(sheet_dump(&sheets));
    }

    #[test]
    fn single_sided_layouts_paginate_with_repeated_headers() {
        let mut sections = Vec::new();
        sections.push(PrintSection {
            kind: SectionKind::Scheduled,
            items: (0..120)
                .map(|n| PrintItem {
                    title: format!("Aufgabe {n:03}"),
                    done: false,
                    time: None,
                    effort: None,
                })
                .collect(),
        });
        let day = PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections,
        };
        for layout in [PrintLayout::A4, PrintLayout::A5, PrintLayout::A6] {
            let sheets = layout_day(&day, layout, &FixedWidth(0.6));
            assert!(sheets.len() > 1, "{layout:?} must paginate 120 items");
            for sheet in &sheets {
                let first = sheet.ops.first().expect("a continued sheet is not empty");
                let DrawOp::Text {
                    text,
                    size_pt,
                    weight,
                    ..
                } = first
                else {
                    panic!("{layout:?} sheet opens with the header, got {first:?}");
                };
                assert_eq!(text, "2026-11-07 Sat");
                assert_eq!(*weight, Weight::Bold);
                assert_eq!(*size_pt, layout.spec().heading_pt);
            }
            // No sheet overflows its page, and no op leaves the page.
            let spec = layout.spec();
            for sheet in &sheets {
                for op in &sheet.ops {
                    let at = match op {
                        DrawOp::Text { at, .. } => *at,
                        DrawOp::Rule { from, to, .. } => min_pt(*from, *to),
                        DrawOp::Checkbox { at, side_pt, .. } => {
                            Pt::new(at.x + side_pt, at.y + side_pt)
                        }
                    };
                    prop_assert_within_page(at, spec.page);
                }
            }
        }
    }

    fn prop_assert_within_page(at: Pt, page: PageSize) {
        assert!(at.x >= -0.5 && at.x <= page.w_pt() + 0.5, "{at:?} off-page");
        assert!(at.y >= -0.5 && at.y <= page.h_pt() + 0.5, "{at:?} off-page");
    }

    #[test]
    fn fold_overflow_yields_one_sheet_and_the_more_marker() {
        let mut sections = Vec::new();
        let items: Vec<PrintItem> = (0..200)
            .map(|n| PrintItem {
                title: format!("item {n:03}"),
                done: false,
                time: None,
                effort: None,
            })
            .collect();
        sections.push(PrintSection {
            kind: SectionKind::Scheduled,
            items,
        });
        let day = PrintDay {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
            sections,
        };
        let sheets = layout_day(&day, PrintLayout::FoldA6x4, &FixedWidth(0.6));
        assert_eq!(sheets.len(), 1);
        let last = sheets[0].ops.last().expect("the marker op exists");
        let DrawOp::Text { text, panel, .. } = last else {
            panic!("last op is the marker text, got {last:?}");
        };
        assert_eq!(*panel, fold_panel_xf(FoldPanel::Four, A4));
        // The correct N: every omitted item is counted; the drawn ones
        // are the item titles that made it onto the sheet.
        let drawn = sheets[0]
            .ops
            .iter()
            .filter(|op| matches!(op, DrawOp::Text { text, .. } if text.starts_with("item ")))
            .count();
        // The omitted count is exact: every block that did not draw
        // (items, plus the trailing Notes heading; the section heading
        // itself always draws) is summed into N.
        assert_eq!(*text, format!("+{} more", 201 - drawn));
        assert!(drawn < 200);
    }

    fn app_state_with(tasks: &[taskboard_domain::Task]) -> AppState {
        let mut state = AppState::default();
        for t in tasks {
            state.tasks.insert(t.id, t.clone());
        }
        state
    }

    fn stamp(secs: i64) -> chrono::DateTime<chrono::Utc> {
        use chrono::TimeZone as _;
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task(
        raw: u128,
        order: i64,
        title: &str,
        duedate: Option<chrono::DateTime<chrono::Utc>>,
        scheduled: Option<chrono::DateTime<chrono::Utc>>,
        done: Option<chrono::DateTime<chrono::Utc>>,
    ) -> taskboard_domain::Task {
        use taskboard_domain::entities::TaskKind;
        use taskboard_domain::{StackId, TaskClocks, TaskId};
        taskboard_domain::Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: title.to_owned(),
            description: String::new(),
            duedate,
            scheduled,
            done,
            stack: StackId::from(uuid::Uuid::from_u128(1)),
            order,
            label_memberships: BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: stamp(100),
                description: stamp(100),
                duedate: stamp(100),
                scheduled: stamp(100),
                done: stamp(100),
                position: stamp(100),
                effort: stamp(100),
                importance: stamp(100),
                urgency: stamp(100),
                recur: stamp(100),
                kind: stamp(100),
                archived: stamp(100),
                deleted: stamp(100),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn calendar() -> DayCalendar {
        DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("valid time"),
            offset: chrono::FixedOffset::east_opt(3600).expect("valid offset"),
        }
    }

    /// Routing only: sections and order come from the domain's
    /// `today_sections`; the phrases are absolute (`HH:MM`, the
    /// overdue `dd.mm.` prefix), never the relative presentation
    /// phrases. Classification itself is `days.rs`'s tested contract.
    #[test]
    fn print_day_projects_sections_in_display_order() {
        let tz = chrono::FixedOffset::east_opt(3600).expect("valid offset");
        let at = |h: u32, m: u32, d: u32| {
            use chrono::TimeZone as _;
            chrono::Utc
                .with_ymd_and_hms(2026, 11, d, h - 1, m, 0)
                .single()
                .expect("valid instant")
        };
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date");
        let now = at(12, 0, 7);
        let state = app_state_with(&[
            // Overdue since yesterday 09:00 local.
            task(1, 1, "old", Some(at(9, 0, 6)), None, None),
            // Scheduled today 09:30 local.
            task(2, 2, "planned", None, Some(at(9, 30, 7)), None),
            // Due today 18:00 local.
            task(3, 3, "due", Some(at(18, 0, 7)), None, None),
            // Done today 07:00 local.
            task(4, 4, "did", None, None, Some(at(7, 0, 7))),
            // Live but outside every section (scheduled tomorrow).
            task(5, 5, "later", None, Some(at(9, 0, 8)), None),
        ]);
        let day = print_day(&state, now, calendar());
        assert_eq!(day.date, today);
        let kinds: Vec<SectionKind> = day.sections.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SectionKind::Overdue,
                SectionKind::Scheduled,
                SectionKind::DueToday,
                SectionKind::Done
            ]
        );
        assert_eq!(day.sections[0].items[0].title, "old");
        // The overdue item carries the `dd.mm.` prefix (its due date is
        // not today) and the absolute due time.
        assert_eq!(
            day.sections[0].items[0].time.as_deref(),
            Some("06.11. 09:00")
        );
        assert_eq!(day.sections[1].items[0].time.as_deref(), Some("09:30"));
        assert_eq!(day.sections[2].items[0].time.as_deref(), Some("18:00"));
        assert!(day.sections[3].items[0].done);
        assert_eq!(day.sections[3].items[0].time.as_deref(), Some("07:00"));
        let _ = tz;
    }

    /// An empty section never draws (the struct is omitted at
    /// projection time).
    #[test]
    fn print_day_omits_empty_sections() {
        let now = {
            use chrono::TimeZone as _;
            chrono::Utc
                .with_ymd_and_hms(2026, 11, 7, 11, 0, 0)
                .single()
                .expect("valid instant")
        };
        let state = app_state_with(&[]);
        let day = print_day(&state, now, calendar());
        assert_eq!(day.sections.len(), 0);
    }

    #[test]
    fn overdue_time_phrase_prefixes_only_foreign_dates() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date");
        let cal = calendar();
        let today_due = cal.instant(today, chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("t"));
        assert_eq!(overdue_time_phrase(today_due, today, cal), "09:00");
        let yesterday = today - chrono::Days::new(1);
        let old = cal.instant(
            yesterday,
            chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("t"),
        );
        assert_eq!(overdue_time_phrase(old, today, cal), "06.11. 09:00");
    }

    #[test]
    fn header_phrase_carries_iso_date_and_weekday() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("a Saturday");
        assert_eq!(header_phrase(today), "2026-11-07 Sat");
    }

    proptest! {
        /// No op of a fold panel leaves its quadrant, and no text op's
        /// measured width passes its panel's right margin (the plan §8
        /// property pair, over random content).
        #[test]
        fn fold_ops_stay_inside_their_panels(
            n_overdue in 0..5usize,
            n_scheduled in 0..30usize,
        ) {
            let m = FixedWidth(0.6);
            let mk = |kind, n: usize| PrintSection {
                kind,
                items: (0..n).map(|i| PrintItem {
                    title: format!("Aufgabe {i} mit ein paar weiteren Wörtern"),
                    done: false,
                    time: Some("09:00".to_owned()),
                    effort: None,
                }).collect(),
            };
            let mut sections = Vec::new();
            if n_overdue > 0 { sections.push(mk(SectionKind::Overdue, n_overdue)); }
            if n_scheduled > 0 { sections.push(mk(SectionKind::Scheduled, n_scheduled)); }
            let day = PrintDay {
                date: chrono::NaiveDate::from_ymd_opt(2026, 11, 7).expect("valid date"),
                sections,
            };
            let spec = PrintLayout::FoldA6x4.spec();
            for sheet in layout_day(&day, PrintLayout::FoldA6x4, &m) {
                for op in &sheet.ops {
                    let (at, panel): (Pt, PanelXf) = match op {
                        DrawOp::Text { at, panel, text, size_pt, .. } => {
                            let right = at.x + m.advance_pt(text, *size_pt);
                            prop_assert!(right <= spec.page.w_pt() / 2.0 + 0.5,
                                "text right edge {right} passes the panel margin");
                            (*at, *panel)
                        }
                        DrawOp::Rule { from, to, panel, .. } => (min_pt(*from, *to), *panel),
                        DrawOp::Checkbox { at, side_pt, panel, .. } => {
                            (Pt::new(at.x + side_pt, at.y + side_pt), *panel)
                        }
                    };
                    let sheet_pt = panel.apply(at);
                    let w = spec.page.w_pt();
                    let h = spec.page.h_pt();
                    prop_assert!(sheet_pt.x >= -0.5 && sheet_pt.x <= w + 0.5);
                    prop_assert!(sheet_pt.y >= -0.5 && sheet_pt.y <= h + 0.5);
                }
            }
        }
    }

    fn min_pt(a: Pt, b: Pt) -> Pt {
        Pt::new(a.x.min(b.x), a.y.min(b.y))
    }
}
