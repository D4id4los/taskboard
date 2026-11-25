// SPDX-License-Identifier: AGPL-3.0-only

//! The layout-severing census (#1252, round-6 chunk E1): every
//! `.slint` file under `ui/views/` is enumerated from the filesystem
//! (a new view file enters census without touching this test), and
//! every exported component in a covered file is either a severing
//! root — its root property block states
//! `max-width: Theme.layout-severing-max-width` and its
//! `<stem>_window_max_probe` twin exists (probe `.slint` — a member
//! of the #1362 derived registry, which compiles every `tests/*.slint`
//! verbatim — plus the asserted probe tests module) — or a witnessed
//! exemption in `EXEMPTIONS`.
//!
//! The failure mode is a missing line of markup at the wrong depth
//! (the #1197 class: a descendant's `max-width` does not satisfy the
//! census; the pre-fix window collapse clamped the OS window to a
//! row's content width). Source-scan, not compiler introspection —
//! the same technique as the sibling `view_mount_inventory`, via the
//! shared `source_scan` module.
//!
//! Boundary: `ui/views/` only. The top-level `ui/*.slint` windows are
//! out of boundary for this chunk — `ui/alarm-window.slint` carries
//! the sever + `alarm_window_max_probe` (#1251, PR #1354, pinned by
//! `alarm_window_structural` + its probe tests), and is admitted as a
//! witnessed out-of-boundary registry entry (`OUT_OF_BOUNDARY_PROBES`);
//! `ui/app-window.slint` is severed + twin since #1378, witnessed by
//! `app_window_max_probe_tests` (the probe itself discharges the
//! issue's witnessed-safety-analysis ask).
//!
//! Two-way evidence (the scratch-checkout pattern of commit
//! `1276151a`, observed 2026-11-24): with the sever line deleted from
//! `ui/views/stack-view.slint` in a scratch checkout, the real-tree
//! census fails with exactly one `MissingSever { file:
//! "stack-view.slint" }`; the negative-case fixtures below pin the
//! same typed variants synthetically (red→green observed before the
//! census core was implemented — the fixtures ran against an empty
//! stub first and every one failed).
//!
//! Golden-census note (#1334, dated 2026-11-25): the today goldens'
//! post-#1310/#1304 vertical spacing offset was a REGRESSION, not
//! acceptable re-spacing — the content layout's constraint solver
//! spilled the Flickable viewport's surplus into the visible
//! stretch-0 children once the empty sections' clip heights became
//! bound (the pre-fix free variables absorbed it invisibly). Fixed by
//! the content layout sink (today-view.slint); the kiosk-today and
//! today-0.5 goldens were re-recorded evidence-carried (byte-stable
//! across two regens), the comparator tolerance untouched. The
//! manifest schema keeps no note field, so this header carries the
//! verdict (the issue's census-note ask).
//!
//! No textual assertions anywhere: the verdict is the empty
//! `Vec<SeveringViolation>`; each fixture asserts its exact typed
//! variant via `matches!`.

use crate::source_scan::{Blanked, ComponentDecl, declared_components, is_exported};

/// A census violation, typed so the fixer reads the class off the
/// variant, not off prose.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SeveringViolation {
    /// A root component whose root property block does not state the
    /// severing max-width.
    MissingSever { file: String },
    /// A severed root whose `<stem>_window_max_probe` twin is absent
    /// (no probe `.slint` in the derived registry + asserted tests
    /// module).
    MissingProbeTwin {
        file: String,
        expected_probe: String,
    },
    /// A `*_window_max_probe` registry entry that maps to no census
    /// root (a deleted view's leftover twin).
    StaleProbeTwin { probe: String },
    /// An exported component that is neither its file's designated
    /// root nor a witnessed exemption.
    UnclassifiedExport { file: String, component: String },
    /// An exemption whose (file, component) pair no longer matches a
    /// live export — the honesty rule of `view_mount_inventory`.
    StaleExemption { file: String, component: String },
}

/// An exported component deliberately outside the root law, witnessed
/// per the honesty rule: each entry must match a live export in the
/// enumerated tree, or `StaleExemption` fires.
struct Exemption {
    file: &'static str,
    component: &'static str,
    reason: &'static str,
}

const EXEMPTIONS: &[Exemption] = &[Exemption {
    file: "review-walk-overlay.slint",
    component: "ReviewWalkOverlay",
    reason: "a hosted dialog body: the sizing law lives in ModalDialogHost (the host owns the scrim, the absorbing panel root, and the sizing law — the file's own header); its embedded PlanNextDayView mount is itself a severed census root",
}];

/// build.rs twin entries whose severed root lives outside the
/// `ui/views/` census boundary, each witnessed by an existing
/// inventory member (here: `alarm_window_structural` + the alarm
/// window's probe tests pin #1251's landed sever, and
/// `app_window_max_probe_tests` witnesses #1378's sever on the
/// `AppWindow` Window root).
const OUT_OF_BOUNDARY_PROBES: &[&str] = &["alarm_window_max_probe", "app_window_max_probe"];

/// The constant the sever states, one string in the guard: a Theme
/// rename fails the census loudly (fail-closed), and the fixer
/// updates one line.
const SEVER_CONSTRAINT: &str = "layout-severing-max-width";

/// The twin name derived from a view file name: the file stem, its
/// trailing `-view` suffix dropped, dashes as underscores
/// (`task-view.slint` → `task_window_max_probe` — uniform since the
/// D3 rename; `habit-history-view.slint` →
/// `habit_history_window_max_probe`).
fn expected_probe(file: &str) -> String {
    let stem = file.strip_suffix(".slint").unwrap_or(file);
    let stem = stem.strip_suffix("-view").unwrap_or(stem);
    format!("{}_window_max_probe", stem.replace('-', "_"))
}

/// Twin names that predate their file's final name, witnessed by the
/// chunk plan's inventory table: an alias keeps the derivation law
/// honest without renaming landed probes.
const TWIN_ALIASES: &[(&str, &str)] = &[("plan-next-day-view.slint", "plan_window_max_probe")];

fn expected_probe_for(file: &str) -> String {
    if let Some((_, probe)) = TWIN_ALIASES.iter().find(|(f, _)| *f == file) {
        return (*probe).to_owned();
    }
    expected_probe(file)
}

/// Whether the root component's property block (depth 1 inside the
/// component body — a descendant's binding does not satisfy the
/// census, which is the #1197 semantics) states
/// `max-width: Theme.layout-severing-max-width`.
fn sever_present(src: &Blanked, root: &ComponentDecl) -> bool {
    let body = root.body_open + 1..root.body_end;
    let prop_depth = src.depth[root.body_open] + 1;
    let tokens = src.tokens(body);
    for (i, (start, token)) in tokens.iter().enumerate() {
        if *token != "max-width" || src.depth[*start] != prop_depth {
            continue;
        }
        // The binding runs to its terminating `;` at the property
        // level; the constraint must appear as `Theme` immediately
        // followed by the severing constant inside it.
        let mut prev_was_theme = false;
        for (s, t) in &tokens[i + 1..] {
            if *t == ";" && src.depth[*s] <= prop_depth {
                break;
            }
            if prev_was_theme && *t == SEVER_CONSTRAINT {
                return true;
            }
            prev_was_theme = *t == "Theme";
        }
    }
    false
}

/// The `*_window_max_probe` stems in the DERIVED probe registry
/// (#1362): every `tests/*.slint` file is a probe — build.rs compiles
/// the `read_dir` enumeration verbatim, so the disk enumeration IS the
/// registry. The census reads the same source of truth, not a stale
/// literal array.
fn probe_registry(manifest: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(manifest.join("tests")) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(stem) = name
            .to_str()
            .and_then(|n| n.strip_suffix(".slint"))
            .map(str::to_owned)
        else {
            continue;
        };
        if stem.ends_with("_window_max_probe") {
            out.push(stem);
        }
    }
    out.sort();
    out
}

fn census(
    files: &[(String, String)],
    registry: &[String],
    present_twins: &[String],
) -> Vec<SeveringViolation> {
    let mut violations = Vec::new();
    let mut roots: Vec<(String, String)> = Vec::new(); // (file, expected probe)
    for (file, source) in files {
        let src = Blanked::blank(source);
        let comps = declared_components(&src);
        let exported: Vec<&ComponentDecl> = comps.iter().filter(|c| is_exported(&src, c)).collect();
        if exported.is_empty() {
            continue;
        }
        // The designated root: the exported component whose block the
        // scanner closes last (the file's own surface). Every other
        // export must be a witnessed exemption.
        let Some(root) = exported.iter().max_by_key(|c| c.body_end) else {
            continue;
        };
        for comp in &exported {
            if std::ptr::eq(*comp, *root) {
                continue;
            }
            let exempt = EXEMPTIONS
                .iter()
                .any(|e| e.file == file && e.component == comp.name);
            if !exempt {
                violations.push(SeveringViolation::UnclassifiedExport {
                    file: file.clone(),
                    component: comp.name.clone(),
                });
            }
        }
        let root_exempt = EXEMPTIONS
            .iter()
            .any(|e| e.file == file && e.component == root.name);
        if root_exempt {
            continue;
        }
        if !sever_present(&src, root) {
            violations.push(SeveringViolation::MissingSever { file: file.clone() });
        }
        let probe = expected_probe_for(file);
        // The twin halves under the derived registry (#1362): the
        // probe `.slint` exists on disk (the registry IS the disk
        // enumeration, and the derivation compiles it verbatim) and
        // the asserted tests module exists beside it.
        let twin_ok =
            registry.iter().any(|t| t == &probe) && present_twins.iter().any(|t| t == &probe);
        if !twin_ok {
            violations.push(SeveringViolation::MissingProbeTwin {
                file: file.clone(),
                expected_probe: probe.clone(),
            });
        }
        roots.push((file.clone(), probe));
    }
    // Reverse census: every `*_window_max_probe` registry entry maps
    // back to a census root or a witnessed out-of-boundary root.
    for probe in registry {
        let mapped = roots.iter().any(|(_, p)| p == probe)
            || OUT_OF_BOUNDARY_PROBES.contains(&probe.as_str());
        if !mapped {
            violations.push(SeveringViolation::StaleProbeTwin {
                probe: probe.clone(),
            });
        }
    }
    // Honesty: each exemption names a live export in the covered
    // file. (A fully absent file is flagged by the real-tree caller,
    // which always enumerates the whole directory.)
    for e in EXEMPTIONS {
        let Some((_, source)) = files.iter().find(|(f, _)| f == e.file) else {
            continue;
        };
        let src = Blanked::blank(source);
        let live = declared_components(&src)
            .iter()
            .filter(|c| is_exported(&src, c))
            .any(|c| c.name == e.component);
        if !live {
            violations.push(SeveringViolation::StaleExemption {
                file: e.file.to_owned(),
                component: e.component.to_owned(),
            });
        }
    }
    violations
}

// --- synthetic fixtures (the negative cases; plan D4) ----------------

/// A one-export synthetic view root, severed or not.
fn fixture_source(severed: bool) -> String {
    let sever = if severed {
        "max-width: Theme.layout-severing-max-width;\n"
    } else {
        ""
    };
    format!("export component FixtureView inherits FocusScope {{\n{sever}}}\n")
}

#[test]
fn an_unsevered_view_root_reports_missing_sever() {
    let files = vec![("fixture-view.slint".to_owned(), fixture_source(false))];
    let probe = "fixture_window_max_probe";
    let violations = census(&files, &[probe.to_owned()], &[probe.to_owned()]);
    assert!(
        matches!(&violations[..], [SeveringViolation::MissingSever { file }] if file == "fixture-view.slint"),
        "expected exactly MissingSever, got {violations:?}"
    );
}

#[test]
fn a_severed_root_without_a_probe_twin_reports_missing_probe_twin() {
    let files = vec![("fixture-view.slint".to_owned(), fixture_source(true))];
    // No build.rs entry, no twin on disk.
    let violations = census(&files, &[], &[]);
    assert!(
        matches!(&violations[..], [SeveringViolation::MissingProbeTwin { file, expected_probe }]
            if file == "fixture-view.slint" && expected_probe == "fixture_window_max_probe"),
        "expected exactly MissingProbeTwin, got {violations:?}"
    );
}

#[test]
fn a_severed_root_twin_needs_both_the_registry_entry_and_the_disk_twin() {
    let files = vec![("fixture-view.slint".to_owned(), fixture_source(true))];
    let probe = "fixture_window_max_probe";
    // Registry entry only — the asserted disk twin is missing.
    let violations = census(&files, &[probe.to_owned()], &[]);
    assert!(
        matches!(
            &violations[..],
            [SeveringViolation::MissingProbeTwin { .. }]
        ),
        "expected MissingProbeTwin when only the registry carries the twin, got {violations:?}"
    );
    // Disk twin only — the registry entry is missing.
    let violations = census(&files, &[], &[probe.to_owned()]);
    assert!(
        matches!(
            &violations[..],
            [SeveringViolation::MissingProbeTwin { .. }]
        ),
        "expected MissingProbeTwin when only the disk carries the twin, got {violations:?}"
    );
    // Both present: the root is fully compliant, no violations.
    let violations = census(&files, &[probe.to_owned()], &[probe.to_owned()]);
    assert!(
        violations.is_empty(),
        "a severed, twinned root is compliant, got {violations:?}"
    );
}

#[test]
fn a_registry_twin_for_a_missing_view_reports_a_stale_probe_twin() {
    let files = vec![("fixture-view.slint".to_owned(), fixture_source(true))];
    let probe = "fixture_window_max_probe";
    let registry = vec![probe.to_owned(), "ghost_window_max_probe".to_owned()];
    let violations = census(&files, &registry, &[probe.to_owned()]);
    assert!(
        matches!(&violations[..], [SeveringViolation::StaleProbeTwin { probe }] if probe == "ghost_window_max_probe"),
        "expected exactly StaleProbeTwin for the ghost twin, got {violations:?}"
    );
}

#[test]
fn an_exported_non_root_component_without_an_exemption_is_unclassified() {
    // Two exports, neither exempt: the last-closed one is the
    // designated root, the earlier one is unclassified.
    let source = "export component SmallWidget inherits Rectangle {\n}\nexport component FixtureView inherits FocusScope {\nmax-width: Theme.layout-severing-max-width;\n}\n";
    let files = vec![("fixture-view.slint".to_owned(), source.to_owned())];
    let probe = "fixture_window_max_probe";
    let violations = census(&files, &[probe.to_owned()], &[probe.to_owned()]);
    assert!(
        matches!(&violations[..], [SeveringViolation::UnclassifiedExport { file, component }]
            if file == "fixture-view.slint" && component == "SmallWidget"),
        "expected exactly UnclassifiedExport, got {violations:?}"
    );
}

#[test]
fn a_stale_exemption_fails_the_honesty_rule() {
    // A file named like an exempt file whose exports no longer match
    // the exemption table: both table entries go stale.
    let source = fixture_source(true);
    let files = vec![("review-walk-overlay.slint".to_owned(), source)];
    let probe = "review_walk_overlay_window_max_probe";
    let violations = census(&files, &[probe.to_owned()], &[probe.to_owned()]);
    let stale: Vec<&str> = violations
        .iter()
        .filter_map(|v| match v {
            SeveringViolation::StaleExemption { component, .. } => Some(component.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        stale.len(),
        1,
        "the exemption entry goes stale: {violations:?}"
    );
    assert!(stale.contains(&"ReviewWalkOverlay"));
}

#[test]
fn every_exemption_carries_a_witness_reason() {
    assert!(EXEMPTIONS.iter().all(|e| !e.reason.is_empty()));
}

// --- the real tree ---------------------------------------------------

/// The twin names present on disk: a probe `.slint` in `tests/` with
/// an asserted tests module in `tests/probes/`.
fn present_twins(manifest: &std::path::Path) -> Vec<String> {
    let tests = manifest.join("tests");
    let probes = tests.join("probes");
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&tests) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(stem) = name
            .to_str()
            .and_then(|n| n.strip_suffix(".slint"))
            .map(str::to_owned)
        else {
            continue;
        };
        if probes.join(format!("{stem}_tests.rs")).exists() {
            out.push(stem);
        }
    }
    out
}

/// The census over the real tree: every `ui/views/*.slint` enumerated
/// from the filesystem, build.rs read as source, every violation
/// collected and the empty vec asserted — the typed verdict.
#[test]
fn every_view_root_is_severed_and_twinned() {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let views = manifest.join("ui").join("views");
    let mut files: Vec<(String, String)> = std::fs::read_dir(&views)
        .expect("the ui/views directory exists")
        .flatten()
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|n| n.to_uppercase().ends_with(".SLINT"))
        .map(|n| {
            let source =
                std::fs::read_to_string(views.join(&n)).expect("the view source is readable");
            (n, source)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let registry = probe_registry(manifest);
    let twins = present_twins(manifest);
    let mut violations = census(&files, &registry, &twins);
    // Honesty, absent-file half: every exemption's file is still
    // enumerated (the exemption table may not outlive its file).
    for e in EXEMPTIONS {
        if !files.iter().any(|(f, _)| f == e.file) {
            violations.push(SeveringViolation::StaleExemption {
                file: e.file.to_owned(),
                component: e.component.to_owned(),
            });
        }
    }
    assert!(
        violations.is_empty(),
        "the severing census found violations: {violations:#?}"
    );
}
