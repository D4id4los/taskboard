#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# The r4 chunk-4 fresh-boot hover repro (GUI review round 4, plan
# §4.2): the F4 "shortcut hints missing from hover boxes early in the
# session" investigation's Tier-1 protocol, plus the D13 empty-line
# and D14 ghost-tooltip observations the same sweep captures.
#
# METHOD (Tier-1, needs an interactive display + xdotool + import):
#   1. Isolation: a scratch config and scratch data/XDG dirs so the
#      boot is cold — never the operator's own store:
#        [ui.keymap] preset = "emacs"   (lowercase serde, config.rs)
#        [storage]  db_path = "$SCRATCH/scratch.db"   (full store
#          isolation; config.rs [storage] table)
#   2. Sweep the named elements in order, pointer parked on each past
#      the 600 ms delay: toolbar menu button, board-bar segment, stack
#      header, task card, add-task ghost, add-column ghost. For each:
#      screenshot (import -window ...) + structured dump of
#      TooltipState (text, keys length, tip-width/tip-height) — never
#      message-text parsing.
#   3. Phases: sweep A straight after the first frame (no
#      interaction); ONE bounded interaction (a single bound key);
#      sweep B. The A↔B delta is F4's "appeared later" made
#      observable. Discriminates H-a (chain masking at mount).
#   4. Settings probe: sweep A → open settings → close → sweep C.
#      Hints changing exactly here revive H-c (the settings-reply
#      keymap swap).
#
# STATUS ON THIS BASE (2026-11-02, stage-2 record): Tier-1 did NOT
# run — the machine's only display is the operator's live KDE/Wayland
# session (XWayland :0), synthetic pointer motion would hijack the
# operator's cursor mid-session, and no nested/isolated server
# (Xvfb/weston) is installed. Per the dispatch prompt, the verdict
# rests on the Tier-4 fallback (the harness tests
# first_frame_hints_match_the_keymap_projection and
# projected_hint_bindings_per_preset, plus the code-evidence chain in
# the stage-2 report) — this script stays committed as the method of
# record for a future interactive run.
#
# TIER-4 (what stage 2 actually ran):
#   cargo nextest run -p taskboard-ui-slint \
#     -E 'test(first_frame_hints_match_the_keymap_projection) or
#         test(projected_hint_bindings_per_preset)'
#
# Usage (on a machine with an isolated display):
#   scripts/r4-hover-repro.sh <path-to-taskboard-release-binary>

set -euo pipefail

BIN="${1:?usage: r4-hover-repro.sh <taskboard-binary>}"
command -v xdotool >/dev/null || {
    echo "xdotool required for the pointer sweep" >&2
    exit 1
}
command -v import >/dev/null || {
    echo "import (ImageMagick) required for the screenshots" >&2
    exit 1
}

SCRATCH="$(mktemp -d /tmp/r4-hover-repro.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
cat > "$SCRATCH/taskboard.toml" <<TOML
[storage]
db_path = "$SCRATCH/scratch.db"

[ui.keymap]
preset = "emacs"
TOML

export TASKBOARD_CONFIG="$SCRATCH/taskboard.toml"
export XDG_DATA_HOME="$SCRATCH/xdg-data"
export XDG_CONFIG_HOME="$SCRATCH/xdg-config"

shot() { import -window root "$SCRATCH/$1.png"; }

echo "boot: cold scratch store, emacs preset; window: taskboard gui"
"$BIN" gui &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null; rm -rf "$SCRATCH"' EXIT
sleep 3  # first frame

# Sweep A: the six named elements, hovered one at a time, pointer
# parked past the 600 ms delay. Coordinates are session-specific —
# resolve them from the window tree (xwininfo) or the a11y tree in a
# real run; the placeholders below are the sweep ORDER of record:
#   toolbar menu button, board-bar segment, stack header, task card,
#   add-task ghost, add-column ghost.
for element in menu-button board-segment stack-header task-card \
    add-task-ghost add-column-ghost; do
    read -r X Y <<<"0 0" # resolve per window geometry in a real run
    xdotool mousemove "$X" "$Y"
    sleep 1 # the 600 ms delay plus margin
    shot "sweepA-$element"
done

# One bounded interaction (H-a discriminator), then sweep B.
xdotool key Escape
for element in menu-button board-segment stack-header task-card \
    add-task-ghost add-column-ghost; do
    read -r X Y <<<"0 0"
    xdotool mousemove "$X" "$Y"
    sleep 1
    shot "sweepB-$element"
done

# The settings probe (H-c discriminator).
xdotool key ctrl+comma
sleep 1
xdotool key ctrl+comma
for element in menu-button board-segment stack-header task-card \
    add-task-ghost add-column-ghost; do
    read -r X Y <<<"0 0"
    xdotool mousemove "$X" "$Y"
    sleep 1
    shot "sweepC-$element"
done

echo "artifacts in $SCRATCH (removed on exit; copy out what you need)"
echo "verdict rule: a hint delta A↔B = H-a; a delta only after settings = H-c;"
echo "a constant keyless board-segment hint = H-b by-design (D19); a hint row"
echo "that changes when the D13 phantom-line fix lands = H-d."
