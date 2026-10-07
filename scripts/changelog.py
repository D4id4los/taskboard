#!/usr/bin/env python3
# CHANGELOG.md tooling for the release pipeline.
#
# Subcommands:
#   promote <new-version> <date>  turn the `## [Unreleased]` section into a
#                                 versioned `## [X.Y.Z] - <date>` section
#   extract <version>             print the `## [X.Y.Z]` section (release-body
#                                 source for CI and local dry-runs)
#   self-test                     run golden tests over both operations;
#                                 non-zero exit on any mismatch
#
# Parsing rule shared by both operations: only *full-line* `^## ` headings
# bound a section, so `### ` subsections never terminate or corrupt one.
# Keep-a-Changelog layout: the promoted section is inserted directly after
# the (now empty) `## [Unreleased]` heading, not nested under its content.

import re
import sys

UNRELEASED_HEADING = "## [Unreleased]"

# A section heading is a full line starting with exactly two '#' and a space.
HEADING_RE = re.compile(r"^## .*$", re.M)


def find_section(text, heading_pattern):
    """Return (start, end, body) of the section whose heading line matches.

    `start` is the offset of the heading line's start, `end` the offset of
    the next full-line `## ` heading (or end of text), `body` the text
    between the heading and `end`.
    """
    match = re.search(heading_pattern, text, re.M)
    if match is None:
        return None
    start = match.start()
    nxt = HEADING_RE.search(text, match.end())
    end = nxt.start() if nxt else len(text)
    return start, end, text[match.end():end]


def promote(text, version, date):
    """Promote the Unreleased content to `## [version] - date`.

    Returns (new_text, promoted: bool). An empty Unreleased section is a
    no-op (never a crash, never a half-written file); the caller decides
    whether an empty release is fatal.
    """
    section = find_section(text, re.escape(UNRELEASED_HEADING) + r"[ \t]*$")
    if section is None:
        raise SystemExit("error: no '## [Unreleased]' section found")
    start, end, body = section
    content = body.strip("\n")
    if not content.strip():
        print("warning: '## [Unreleased]' section is empty; nothing to promote", file=sys.stderr)
        return text, False
    release = f"## [{version}] - {date}"
    tail = text[end:]
    if not tail:
        # No following heading: the promoted section ends the file.
        new_text = (
            text[:start]
            + UNRELEASED_HEADING
            + "\n\n"
            + release
            + "\n\n"
            + content
            + "\n"
        )
    else:
        new_text = (
            text[:start]
            + UNRELEASED_HEADING
            + "\n\n"
            + release
            + "\n\n"
            + content
            + "\n\n"
            + tail
        )
    return new_text, True


def extract(text, version):
    """Return the `## [version]` section body (without the heading line)."""
    section = find_section(text, re.escape(f"## [{version}]") + r"(?:[ \t-].*)?$")
    if section is None:
        raise SystemExit(f"error: no '## [{version}]' section found")
    return section[2].strip("\n") + "\n"


def promote_golden(text, version, date):
    new_text, promoted = promote(text, version, date)
    return new_text, promoted


GOLDEN_PROMOTE_INPUT = """# Changelog

## [Unreleased]

### Fixed

- something broken got better
### Added

- a thing

## [0.1.0] - 2026-01-01

### Added

- initial release
"""

GOLDEN_PROMOTE_EXPECTED = """# Changelog

## [Unreleased]

## [0.2.0] - 2026-10-08

### Fixed

- something broken got better
### Added

- a thing

## [0.1.0] - 2026-01-01

### Added

- initial release
"""

GOLDEN_PROMOTE_EMPTY_INPUT = """# Changelog

## [Unreleased]

## [0.1.0] - 2026-01-01

### Added

- initial release
"""

# Empty Unreleased: unchanged file, no crash.
GOLDEN_PROMOTE_EMPTY_EXPECTED = GOLDEN_PROMOTE_EMPTY_INPUT

GOLDEN_PROMOTE_ONLY_INPUT = """# Changelog

## [Unreleased]

### Fixed

- only section in the file
"""

GOLDEN_PROMOTE_ONLY_EXPECTED = """# Changelog

## [Unreleased]

## [0.2.0] - 2026-10-08

### Fixed

- only section in the file
"""


def self_test():
    failures = []

    def check(name, actual, expected):
        if actual != expected:
            failures.append(name)
            print(f"FAIL: {name}", file=sys.stderr)
            print("--- expected ---\n" + expected + "--- actual ---\n" + actual + "---", file=sys.stderr)
        else:
            print(f"ok: {name}")

    new_text, promoted = promote_golden(GOLDEN_PROMOTE_INPUT, "0.2.0", "2026-10-08")
    check("promote: subsection not corrupted (### stays ###)", new_text, GOLDEN_PROMOTE_EXPECTED)
    check("promote: content promoted", promoted, True)
    check(
        "promote: versioned section after (not under) Unreleased",
        new_text.index("## [0.2.0]") > new_text.index(UNRELEASED_HEADING)
        and new_text.count(UNRELEASED_HEADING) == 1,
        True,
    )

    empty_text, promoted = promote_golden(GOLDEN_PROMOTE_EMPTY_INPUT, "0.2.0", "2026-10-08")
    check("promote: empty Unreleased is a no-op", empty_text, GOLDEN_PROMOTE_EMPTY_EXPECTED)
    check("promote: empty Unreleased reports not-promoted", promoted, False)

    only_text, promoted = promote_golden(GOLDEN_PROMOTE_ONLY_INPUT, "0.2.0", "2026-10-08")
    check("promote: Unreleased as only section", only_text, GOLDEN_PROMOTE_ONLY_EXPECTED)
    check("promote: only-section content promoted", promoted, True)

    # Extraction round-trip: promote, then extract the promoted version.
    extracted = extract(new_text, "0.2.0")
    check("extract: round-trips the promoted section", extracted, "### Fixed\n\n- something broken got better\n### Added\n\n- a thing\n")

    extracted_initial = extract(GOLDEN_PROMOTE_INPUT, "0.1.0")
    check("extract: reads an existing version section", extracted_initial, "### Added\n\n- initial release\n")

    if failures:
        print(f"self-test FAILED: {len(failures)} case(s)", file=sys.stderr)
        return 1
    print("self-test passed")
    return 0


def main(argv):
    usage = (
        "usage: changelog.py promote <new-version> <date> [changelog-path]\n"
        "       changelog.py extract <version> [changelog-path]\n"
        "       changelog.py self-test"
    )
    if argv[1:] == ["self-test"]:
        return self_test()
    if len(argv) in (4, 5) and argv[1] == "promote":
        version, date = argv[2], argv[3]
        path = argv[4] if len(argv) == 5 else "CHANGELOG.md"
        text = open(path, encoding="utf-8").read()
        new_text, promoted = promote(text, version, date)
        if promoted:
            open(path, "w", encoding="utf-8").write(new_text)
        return 0
    if len(argv) in (3, 4) and argv[1] == "extract":
        version = argv[2]
        path = argv[3] if len(argv) == 4 else "CHANGELOG.md"
        text = open(path, encoding="utf-8").read()
        sys.stdout.write(extract(text, version))
        return 0
    print(usage, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
