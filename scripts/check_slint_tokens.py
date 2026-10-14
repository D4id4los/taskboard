#!/usr/bin/env python3
"""Token lint gate for the Slint UI (GUI Phase 7, design §3.4).

Three rules, enforced over every ``*.slint`` file under
``crates/taskboard-ui-slint/`` (any future .slint location joins by
construction — the scanner walks the whole tree):

1. No literal colours outside ``ui/theme.slint``: hex forms
   (``#rgb``, ``#rrggbb``, ``#rrggbbaa``, ``#aarrggbb``), ``rgb(``,
   ``rgba(``, ``hsl(``, and ``Colors.*`` named constants.
2. No literal font attributes outside ``ui/theme.slint``: any
   occurrence of ``font-size``, ``font-weight``, ``font-family``
   (writes and reads — the theme is the only legitimate mention).
3. No literal spacing dimensions in non-theme files: ``px``-suffixed
   literals in expressions bound to layout properties (``width``,
   ``height``, their min-/max- variants, ``x``, ``y``, ``padding*``,
   ``margin*``, ``*-spacing``, ``spacing``) — unless the value
   expression references a theme token (``Theme.``, ``Typography.``,
   ``Semantic.``) or is proportional/relative (percent values, ``fr``
   units). Pure arithmetic over other lengths carries no ``px`` literal
   and passes by construction.

The regex set is conservative on purpose: prefer a miss (a literal
slips through; the reviewer greps new .slint diffs as the second net)
over a false hit (a compliant expression fails).

No suppression escape hatch exists — no comment pragma, no per-file
waivers. Adding one later needs a recorded reason (design §3.4).

Usage: ``check_slint_tokens.py [--self-test]``; exit 0 = clean, 1 =
violations (or self-test failure).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCAN_ROOT = REPO_ROOT / "crates" / "taskboard-ui-slint"
# The single exemptions file (path suffix match): the theme module is the
# only place colours, font attributes, and px literals may appear.
THEME_SUFFIX = "ui/theme.slint"

# Comments are stripped before matching (both slint comment forms).
LINE_COMMENT = re.compile(r"//[^\n]*")
BLOCK_COMMENT = re.compile(r"/\*.*?\*/", re.DOTALL)

# Rule 1: literal colours.
HEX_COLOR = re.compile(r"#[0-9a-fA-F]{3,8}\b")
FUNC_COLOR = re.compile(r"\b(?:rgba?|hsla?)\s*\(")
NAMED_COLOR = re.compile(r"\bColors\s*\.")

# Rule 2: literal font attributes (property writes).
FONT_ATTR = re.compile(r"\bfont-(?:size|weight|family)\s*[:<]?=?")

# Rule 3: px literals bound to layout properties. The binding forms we
# recognise: `prop: expr;`, `prop <=> expr;`, `prop <=> { ... }`.
LAYOUT_PROPS = (
    r"(?:min-|max-)?(?:width|height)"
    r"|x|y"
    r"|(?:padding|margin)(?:-(?:top|bottom|left|right))?"
    r"|(?:[a-z-]+-)?spacing"
)
LAYOUT_BINDING = re.compile(
    rf"\b({LAYOUT_PROPS})\s*(?::|<=>)\s*([^;}}]+)[;}}]",
)
PX_LITERAL = re.compile(r"\b\d+(?:\.\d+)?px\b")
TOKEN_REF = re.compile(r"\b(?:Theme|Typography|Semantic)\s*\.")
PROPORTIONAL = re.compile(r"%|\bfr\b")

# Layout binding may span lines; normalise whitespace per statement.


def strip_comments(text: str) -> str:
    text = BLOCK_COMMENT.sub(" ", text)
    text = LINE_COMMENT.sub(" ", text)
    return text


def normalise(text: str) -> tuple[str, list[int]]:
    """Collapse whitespace so multi-line bindings match as statements.

    Returns the normalised text plus, per normalised character, the
    offset in ``text`` it came from — violation reports cite source
    line numbers, so matches on the collapsed text must map back.
    """
    out: list[str] = []
    offsets: list[int] = []
    in_run = False
    for i, ch in enumerate(text):
        if ch.isspace():
            if not in_run:
                out.append(" ")
                offsets.append(i)
                in_run = True
            continue
        out.append(ch)
        offsets.append(i)
        in_run = False
    return "".join(out), offsets


def scan_text(rel_path: str, text: str) -> list[tuple[int, str]]:
    """Returns (line, description) violations for one file's content.

    ``rel_path`` is the path relative to the scan root, used for the
    theme-file exemption.
    """
    if rel_path.endswith(THEME_SUFFIX):
        return []
    stripped = strip_comments(text)
    normalised, offsets = normalise(stripped)
    violations: list[tuple[int, str]] = []

    def line_of(match: re.Match[str]) -> int:
        return stripped.count("\n", 0, offsets[match.start()]) + 1

    for match in FUNC_COLOR.finditer(normalised):
        violations.append((line_of(match), f"literal colour call: {match.group(0)!r}"))
    for match in NAMED_COLOR.finditer(normalised):
        violations.append((line_of(match), f"named colour constant: {match.group(0)!r}"))
    for match in HEX_COLOR.finditer(normalised):
        # Hex runs may be legitimate non-colour tokens in principle; in
        # .slint sources a `#`-prefixed hex literal *is* the colour
        # syntax, so every hit is a violation.
        violations.append((line_of(match), f"literal colour: {match.group(0)!r}"))
    for match in FONT_ATTR.finditer(normalised):
        violations.append((line_of(match), f"literal font attribute: {match.group(0)!r}"))
    for match in LAYOUT_BINDING.finditer(normalised):
        expr = match.group(2)
        if TOKEN_REF.search(expr) or PROPORTIONAL.search(expr):
            continue
        for px in PX_LITERAL.finditer(expr):
            violations.append(
                (
                    line_of(match),
                    f"literal spacing dimension in {match.group(1)!r}: {px.group(0)!r}",
                )
            )
    return sorted(violations)


def scan_tree(root: Path = SCAN_ROOT) -> list[str]:
    """Scans every *.slint under ``root``; returns formatted violations."""
    problems: list[str] = []
    for path in sorted(root.rglob("*.slint")):
        rel = str(path.relative_to(root))
        for line, description in scan_text(rel, path.read_text(encoding="utf-8")):
            problems.append(f"{rel}:{line}: {description}")
    return problems


# ---------------------------------------------------------------------------
# Self-test: fixtures per rule — a violating snippet per rule fails, the
# theme-file exemption passes, the proportional exemption passes, a
# token-referencing expression passes (the changelog.py pattern).
# ---------------------------------------------------------------------------

def _self_test() -> int:
    failures: list[str] = []

    def expect_violations(name: str, rel: str, text: str, count: int = 1) -> None:
        got = scan_text(rel, text)
        if len(got) != count:
            failures.append(f"{name}: expected {count} violation(s), got {len(got)}: {got}")

    def expect_clean(name: str, rel: str, text: str) -> None:
        got = scan_text(rel, text)
        if got:
            failures.append(f"{name}: expected clean, got {got}")

    # Rule 1: literal colours fail outside the theme.
    expect_violations("rule1-hex", "ui/other.slint", 'rect { background: #ff0000; }')
    expect_violations("rule1-rgb", "ui/other.slint", "rect { background: rgb(10, 20, 30); }")
    expect_violations("rule1-named", "ui/other.slint", "rect { color: Colors.red; }")
    # Rule 1: comments do not smuggle violations in…
    expect_clean("rule1-comment", "ui/other.slint", "// was #ff0000 once\nrect {}")
    expect_clean("rule1-block-comment", "ui/other.slint", "/* rgb(1,2,3) */ rect {}")
    # …and out of the theme file everything literal is fine.
    expect_clean("rule1-theme", "ui/theme.slint", "background: #ff0000;\ncolor: rgb(1,2,3);")

    # Rule 2: font attribute writes fail outside the theme.
    expect_violations("rule2-size", "ui/other.slint", 'Text { font-size: 12px; }')
    expect_violations("rule2-weight", "ui/other.slint", 'Text { font-weight: 700; }')
    expect_clean("rule2-theme", "ui/theme.slint", 'Text { font-size: 12px; }')

    # Rule 3: px literals on layout props fail outside the theme.
    expect_violations("rule3-width", "ui/other.slint", "rect { width: 24px; }")
    expect_violations("rule3-padding", "ui/other.slint", "rect { padding-left: 8px; }")
    expect_violations("rule3-spacing", "ui/other.slint", "VerticalLayout { spacing: 4px; }")
    expect_violations("rule3-multiline", "ui/other.slint", "rect {\n  height:\n    10px;\n}")
    # Reported line numbers cite the source text, even where whitespace
    # collapsing shifted the match offsets (multi-space runs, wrapped
    # bindings): the report points at the binding's first line.
    got = scan_text("ui/other.slint", "rect {\n    width:\n        12px;\n}")
    if got != [(2, "literal spacing dimension in 'width': '12px'")]:
        failures.append(f"rule3-line-report: expected line 2, got {got}")
    # Rule 3 exemptions: token reference, proportional, pure arithmetic
    # over other lengths, and the theme file.
    expect_clean("rule3-token", "ui/other.slint", "rect { width: Theme.space-4; }")
    expect_clean("rule3-token-expr", "ui/other.slint", "rect { width: Theme.space-4 * 2; }")
    expect_clean("rule3-percent", "ui/other.slint", "rect { width: 50%; }")
    expect_clean("rule3-fr", "ui/other.slint", "HorizontalLayout { rect { width: 1fr; } }")
    expect_clean("rule3-arithmetic", "ui/other.slint", "rect { width: parent.width - Theme.space-1; }")
    expect_clean("rule3-theme", "ui/theme.slint", "property <length> s: 4px;")

    # Non-layout px literals (e.g. border-radius-width on a shadow) are
    # deliberately out of rule 3's scope (conservative bias: prefer a
    # miss over a false hit; the reviewer greps diffs).
    expect_clean("rule3-non-layout", "ui/other.slint", "rect { drop-shadow-blur: 4px; }")

    if failures:
        for failure in failures:
            print(f"self-test FAIL: {failure}", file=sys.stderr)
        return 1
    print("self-test OK: all lint fixtures behave as specified")
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return _self_test()
    problems = scan_tree()
    if problems:
        print(f"slint token lint: {len(problems)} violation(s):", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "colours, font attributes, and spacing dimensions live in ui/theme.slint"
            " (design §3.3/§3.4); reference Theme./Typography./Semantic. tokens instead",
            file=sys.stderr,
        )
        return 1
    print("slint token lint: clean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
