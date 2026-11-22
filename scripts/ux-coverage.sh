#!/usr/bin/env bash
# UX journey completeness gate (AGENTS.md §7; docs/ux_planning.org §2).
#
# Parses the fixed-format tables of a UX journey plan (written from
# .artifacts/templates/ux-journey-plan.md) and greps the repository for
# the evidence each row claims:
#   - wiring-inventory rows: the slint callback exists in a .slint file
#     AND is handled in an adapter.rs (on_<callback> / install_seam!);
#     each command / ActionId token exists in messages.rs / requests.rs /
#     actions.rs as the plan's command-contract delta Kind selects; a
#     resolved callback must additionally be listed in
#     seam_inventory.rs (the adapter's structural manifest) — an
#     unlisted callback is INVENTORY-MISS (#760);
#   - journey steps: the snake_case step token (US-1/S2 -> us1_s2)
#     resolves against the plan's own nominations (#920). The §6 Test
#     column nominates the per-plan evidence file
#     (<file>.rs::<token-or-name>); a token named in no Test cell can
#     never be credited from the shared tree (an ambient hit is
#     AMBIGUOUS — triage, never FOUND). Retro plans map pre-existing
#     descriptively-named tests with `bound-test:` lines (#768);
#   - capability-matrix cells citing a story: that story's §4 table
#     exists and every one of its steps is covered;
#   - rulings: every ruling row carries a non-empty reason.
#
# Verdict states (docs/ux_planning.org §2.3): FOUND (file:line) and
# BOUND (file:line) pass; MISSING, AMBIGUOUS (k candidates: …) and
# INVENTORY-MISS (slint:line, adapter:line) are open rows. The
# naming/binding law lives in docs/ux_planning.org §2.3.
#
# HONEST LIMITS: this gate proves wiring exists and is claimed-tested —
# not that the UX is good. Quality is the walkthrough protocol's job
# (docs/ux_planning.org). The two layers together answer "is the UX
# journey fully implemented": layer 1 is mechanical and cheap, layer 2
# is agentic and runs at review time. The gate is a stage-2 self-check
# and a stage-3 mandate input, never CI.
#
# Fail-loudly law: a malformed or missing table is a parse error (exit
# 2, `PARSE-ERROR: <section>: <what>`), never a silently skipped check.
# Cells may be empty or `—`; emptiness is data, not malformation.
#
# Exit codes: 0 = all rows FOUND/BOUND (or plain informational run);
# 1 = a well-formed plan has MISSING/AMBIGUOUS/INVENTORY-MISS rows and
# --gate was given; 2 = parse error; 64 = usage error.
#
# Usage:
#   scripts/ux-coverage.sh <plan.md> [--gate] [--root <dir>]
#   scripts/ux-coverage.sh --self-test
#
# --root <dir>: repository root for evidence greps (default: the git
#   toplevel of the script's location). --self-test points --root at
#   scripts/fixtures/ux-coverage-fixture/tree so PASS rows resolve
#   against the miniature evidence tree.
set -euo pipefail

readonly CAP_ANCHOR='## 2. Capability × surface matrix'
readonly JRN_ANCHOR='## 4. Journey specs'
readonly WIR_ANCHOR='## 6. Wiring inventory'
readonly DEL_ANCHOR='## 7. Command-contract delta'

usage() {
  cat >&2 <<'EOF'
usage: scripts/ux-coverage.sh <plan.md> [--gate] [--root <dir>]
       scripts/ux-coverage.sh --self-test

  <plan.md>      UX journey plan written from the template
  --gate         exit 1 when any row is MISSING, AMBIGUOUS or
                 INVENTORY-MISS (a plain run prints the same table but
                 exits 0 — the report is informational)
  --root <dir>   repository root for evidence greps (default: the git
                 toplevel of the script's location)
  --self-test    exercise the bundled fixture's PASS, BOUND, AMBIGUOUS,
                 INVENTORY-MISS, MISSING and dead-bound-test paths, a
                 malformed-table variant, the #965 boundary-matching
                 variants (prefix-miss and prefix-found), and the
                 #967/#970 exclusion variants; asserts exit codes and
                 row counts only

exit codes: 0 all FOUND/BOUND / informational; 1 open rows under
            --gate; 2 PARSE-ERROR; 64 usage
EOF
}

# ---------------------------------------------------------------------------
# Parser: awk over the plan's pipe tables, keyed on level-2 heading
# anchors (prefix + exact title, never line numbers). Emits records:
#   ROW <table> <extra> <cell>\x1f<cell>...   (extra: story id for journey)
# Any contract violation prints PARSE-ERROR and exits 2.
# ---------------------------------------------------------------------------
parse_plan() {
  awk -v cap_anchor="$CAP_ANCHOR" -v jrn_anchor="$JRN_ANCHOR" \
      -v wir_anchor="$WIR_ANCHOR" -v del_anchor="$DEL_ANCHOR" '
    function fail(sec, what) {
      if (!failed) {
        failed = 1
        printf "PARSE-ERROR: %s: %s\n", sec, what
      }
      exit 2
    }
    function cellcount(line,   s, n) {
      s = line; sub(/^\|/, "", s); sub(/\|[[:space:]]*$/, "", s)
      n = split(s, c, "|")
      return n
    }
    function cellget(line, i,   s) {
      s = line; sub(/^\|/, "", s); sub(/\|[[:space:]]*$/, "", s)
      split(s, c, "|")
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", c[i])
      # (#1181) strip inline code spans wholesale: `` `ActionId::X` ``
      # must parse exactly like the bare token. The plan grammar never
      # nests backticks, so a single-level strip is lossless for every
      # token-bearing cell; prose cells never matched anyway.
      gsub(/`/, "", c[i])
      return c[i]
    }
    function issep(line,   s, n, i) {
      s = line; sub(/^\|/, "", s); sub(/\|[[:space:]]*$/, "", s)
      n = split(s, c, "|")
      if (n < 1) return 0
      for (i = 1; i <= n; i++)
        if (c[i] !~ /^[[:space:]:|-]+$/) return 0
      return 1
    }
    function header_matches(line, want,   n, i, joined) {
      n = cellcount(line)
      if (n != split(want, w, "\x1f")) return 0
      joined = ""
      for (i = 1; i <= n; i++)
        joined = joined (i > 1 ? "\x1f" : "") cellget(line, i)
      return joined == want
    }
    function close_table() {
      if (table != "") {
        if (ndata == 0) fail(section_name, "table with header but no data rows")
        table = ""
      }
    }
    function close_section() {
      close_table()
      if (section == "cap") {
        if (!seen_cap) fail(section_name, "missing capability matrix table")
        if (!seen_rul) fail(section_name, "missing rulings table")
      } else if (section == "jrn") {
        if (nstories == 0) fail(section_name, "no ### US-<n> subsection with a journey table")
      } else if (section == "wir") {
        if (!seen_wir) fail(section_name, "missing wiring table")
      } else if (section == "del") {
        if (!seen_del) fail(section_name, "missing command-delta table")
      }
    }
    BEGIN {
      cap_hdr = "Capability\x1fBoard\x1fTaskDetail\x1fToday\x1fReview\x1fSettings\x1fCapture\x1fCLI"
      rul_hdr = "ID\x1fRuling\x1fReason\x1fHome"
      jrn_hdr = "Step\x1fSurface & affordance\x1fUser action\x1fDispatches\x1fSystem response\x1fEscape / failure"
      wir_hdr = "ID\x1fSurface file\x1fWidget\x1fCallback\x1fAdapter seam\x1fActionId\x1fCommands\x1fRow addressing\x1fJourney steps\x1fTest"
      del_hdr = "Kind\x1fName\x1fPayload sketch\x1fJourneys"
      section = ""; table = ""; ncols = 0; ndata = 0; failed = 0
      seen_cap = 0; seen_rul = 0; seen_wir = 0; seen_del = 0; seen_jrn = 0
      nstories = 0; story = ""
    }
    /^##[[:space:]]/ {
      close_section()
      if ($0 == cap_anchor) { section = "cap"; section_name = "§2 capability matrix" }
      else if ($0 == jrn_anchor) { section = "jrn"; section_name = "§4 journey specs" }
      else if ($0 == wir_anchor) { section = "wir"; section_name = "§6 wiring inventory" }
      else if ($0 == del_anchor) { section = "del"; section_name = "§7 command delta" }
      else section = ""
      next
    }
    /^###[[:space:]]/ {
      close_table()
      if (section == "jrn") {
        if ($0 ~ /^###[[:space:]]+US-[0-9]+/) {
          story = $0
          sub(/^###[[:space:]]+/, "", story)
          sub(/[[:space:]].*$/, "", story)
          nstories++
        } else {
          story = ""
        }
      }
      next
    }
    /^\|/ {
      if (section == "") next
      if (table == "") {
        if (section == "cap") {
          if (header_matches($0, cap_hdr)) { table = "cap"; ncols = 8; seen_cap = 1 }
          else if (header_matches($0, rul_hdr)) { table = "rul"; ncols = 4; seen_rul = 1 }
          else fail(section_name, "unrecognized table header: " $0)
        } else if (section == "jrn") {
          if (story == "") fail(section_name, "journey table outside a ### US-<n> subsection")
          if (header_matches($0, jrn_hdr)) { table = "jrn"; ncols = 6; seen_jrn = 1 }
          else fail(section_name " (story " story ")", "unrecognized journey table header: " $0)
        } else if (section == "wir") {
          if (header_matches($0, wir_hdr)) { table = "wir"; ncols = 10; seen_wir = 1 }
          else fail(section_name, "unrecognized table header: " $0)
        } else if (section == "del") {
          if (header_matches($0, del_hdr)) { table = "del"; ncols = 4; seen_del = 1 }
          else fail(section_name, "unrecognized table header: " $0)
        }
        expect_sep = 1; ndata = 0
        next
      }
      if (expect_sep) {
        if (!issep($0)) fail(section_name, "missing |---| separator row after header")
        expect_sep = 0
        next
      }
      if (issep($0)) fail(section_name, "unexpected separator row inside data")
      if (cellcount($0) != ncols) fail(section_name, "ragged data row (cell count != " ncols "): " $0)
      row = ""
      for (i = 1; i <= ncols; i++)
        row = row (i > 1 ? "\x1f" : "") cellget($0, i)
      extra = (table == "jrn" ? story : "")
      printf "ROW %s %s %s\n", table, extra, row
      ndata++
      next
    }
    {
      # Any other line (blank or not) ends the current table.
      close_table()
    }
    END {
      if (failed) exit 2
      if (section != "") close_section()
      if (!seen_cap || !seen_rul || !seen_jrn || !seen_wir || !seen_del) {
        # The template mandates all four anchored sections (§2 carries
        # two tables); a missing one is a parse error, never a skip.
        miss = ""
        if (!seen_cap) miss = miss " §2-capability-matrix"
        if (!seen_rul) miss = miss " §2-rulings"
        if (!seen_jrn) miss = miss " §4-journey-specs"
        if (!seen_wir) miss = miss " §6-wiring-inventory"
        if (!seen_del) miss = miss " §7-command-delta"
        fail("plan", "missing anchored section(s):" miss)
      }
    }
  ' "$1"
}

# ---------------------------------------------------------------------------
# Evidence search
# ---------------------------------------------------------------------------
FOUND_EV=""

# Token match (#965): boundary-anchored — the token must not be
# preceded or followed by an identifier character, so `save` does not
# match `save_extra` / `on_save_extra` (the #920 substring class, gate
# edition). Tokens are identifier-shaped by the plan grammar, so no
# regex escaping is needed; an honest occurrence in real source is
# always delimited by punctuation, whitespace or end of line, so the
# anchor never turns an honest match into a miss.
tok_match() { # tok_match <token> <file>...
  local tok=$1 f out
  shift
  for f in "$@"; do
    [[ -f $f ]] || continue
    if out=$(grep -nE -- "(^|[^A-Za-z0-9_])${tok}([^A-Za-z0-9_]|$)" "$f" 2>/dev/null | head -n 1); then
      FOUND_EV="${f}:${out%%:*}"
      return 0
    fi
  done
  return 1
}

adapter_match() { # adapter_match <callback> <adapter-file>...
  local cb=$1 f out
  shift
  for f in "$@"; do
    [[ -f $f ]] || continue
    if out=$(grep -nE -- "on_${cb}([^A-Za-z0-9_]|$)" "$f" 2>/dev/null | head -n 1); then
      FOUND_EV="${f}:${out%%:*}"
      return 0
    fi
    if out=$(grep -nF "install_seam!" "$f" 2>/dev/null | grep -E -- "(^|[^A-Za-z0-9_])${cb}([^A-Za-z0-9_]|$)" | head -n 1); then
      FOUND_EV="${f}:${out%%:*}"
      return 0
    fi
  done
  return 1
}

# Evidence universe scan (#967/#970): one shared find predicate —
# prune the self-test fixture root (evidence-scoped: exactly the
# script-relative fixtures/ux-coverage-fixture tree when it exists,
# never a blanket scripts/ exclusion), build-output directories
# (`target`, `node_modules` — no repo source directory collides) and
# `.git`. Uses EVID_ROOT and FX_ROOT from the caller.
find_evidence() { # find_evidence <name>
  local name=$1
  local -a args=(find "$EVID_ROOT")
  if [[ -n ${FX_ROOT:-} ]]; then
    args+=(-path "$FX_ROOT" -prune -o)
  fi
  args+=(-name target -prune -o
         -name node_modules -prune -o
         -path "$EVID_ROOT/.git" -prune -o
         -type f -name "$name" -print)
  # (#1073) determinism: `find` order is arbitrary, and the by-name
  # consumers (first_match / adapter_match / nom_file's fallback) credit
  # the first hit — so a token present in both a production file and a
  # test-support twin resolved on whichever find emitted first. Post-
  # filter into two tiers: tier 0 = paths NOT under a test-support
  # location (a /test_support/ or /tests/ path segment — structural, so
  # new crates' fakes de-prioritize for free), tier 1 = the rest; each
  # tier LC_ALL=C sort -u (byte order, dedup). Test-support files stay
  # in the universe — a token that only exists there still resolves,
  # FOUND citing the test-support file (deliberate leniency, the #965/
  # #967 posture); only the nondeterminism is removed.
  local raw tier0 tier1
  raw=$("${args[@]}" 2>/dev/null || true)
  tier0=$(grep -vE '/(test_support|tests)/' <<<"$raw" | LC_ALL=C sort -u || true)
  tier1=$(grep -E '/(test_support|tests)/' <<<"$raw" | LC_ALL=C sort -u || true)
  [[ -n $tier0 ]] && printf '%s\n' "$tier0"
  [[ -n $tier1 ]] && printf '%s\n' "$tier1"
  return 0
}

# Boundary fn-name match: `fn <name>` not followed by another name
# character — `fn us1_s1` must not match `fn us1_s1_settings_cards`.
# Emits `file:line:fn-line` excerpt lines (callers count and consume
# them; the function itself always returns 0). AMBIGUOUS candidates
# keep the excerpt (triage); crediting evidence is stripped to
# file:line by ev_fileline.
fn_boundary_matches() { # fn_boundary_matches <name> <file>...
  local name=$1 f
  shift
  for f in "$@"; do
    [[ -f $f ]] || continue
    grep -nE -- "fn ${name}(_|\\()" "$f" 2>/dev/null | sed "s|^|${f}:|" || true
  done
  return 0
}

# Strip a `file:line:content` excerpt down to the verdict contract's
# `file:line` evidence (paths in the evidence universe contain no ':').
ev_fileline() { # ev_fileline <file:line:content> -> file:line
  local path=${1%%:*} rest=${1#*:}
  echo "${path}:${rest%%:*}"
}

n_found=0
n_missing=0
n_ambiguous=0
n_bound=0
n_invmiss=0

emit_found() {
  echo "$1 → FOUND ($FOUND_EV)"
  n_found=$((n_found + 1))
}
emit_found_in_plan() {
  echo "$1 → FOUND ($2)"
  n_found=$((n_found + 1))
}
emit_bound() {
  echo "$1 → BOUND ($FOUND_EV)"
  n_bound=$((n_bound + 1))
}
emit_missing() {
  echo "$1 → MISSING"
  n_missing=$((n_missing + 1))
}
emit_ambiguous() { # emit_ambiguous <label> <candidates...>
  local label=$1 k=0 shown=() c
  shift
  for c in "$@"; do
    k=$((k + 1))
    if (( ${#shown[@]} < 5 )); then shown+=("$c"); fi
  done
  echo "$label → AMBIGUOUS (${k} candidates: ${shown[*]})"
  n_ambiguous=$((n_ambiguous + 1))
}
emit_inventory_miss() { # emit_inventory_miss <label> <slint-ev> <adapter-ev>
  echo "$1 → INVENTORY-MISS ($2, $3)"
  n_invmiss=$((n_invmiss + 1))
}

is_absent() { [[ -z $1 || $1 == "—" || $1 == "-" ]]; }

is_bare_token() { [[ $1 =~ ^us[0-9]+_s[0-9]+$ ]]; }

# ---------------------------------------------------------------------------
# Self-test: exit codes and row counts only — never output-text
# assertions (the no-text-assertion rule applies to the tool's own
# tests; FOUND/MISSING/AMBIGUOUS row labels are structure, not prose).
# ---------------------------------------------------------------------------
self_test() {
  local script_dir fx tmp code n rc=0
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  fx="${script_dir}/fixtures/ux-coverage-fixture"
  tmp="$(mktemp -d)"

  # 1. PASS fixture: every step nominated (Test column + one bound-test
  #    line), wiring resolved and listed in the fixture inventory.
  bash "${BASH_SOURCE[0]}" "$fx/plan.md" --gate --root "$fx/tree" \
    >"$tmp/pass.out" 2>&1 && code=0 || code=$?
  n=$(grep -c ' BOUND (' "$tmp/pass.out" || true)
  if [[ $code -eq 0 && $n -eq 1 ]]; then
    echo "self-test 1 (PASS fixture --gate exits 0, exactly 1 BOUND row): PASS"
  else
    echo "self-test 1 (PASS fixture --gate exits 0, exactly 1 BOUND row): FAIL (exit $code, $n BOUND rows)"
    sed 's/^/    /' "$tmp/pass.out"
    rc=1
  fi

  # 2. MISSING fixture: exactly the three deliberate holes, exit 1.
  bash "${BASH_SOURCE[0]}" "$fx/plan-missing.md" --gate --root "$fx/tree" \
    >"$tmp/miss.out" 2>&1 && code=0 || code=$?
  n=$(grep -c 'MISSING$' "$tmp/miss.out" || true)
  if [[ $code -eq 1 && $n -eq 3 ]]; then
    echo "self-test 2 (MISSING fixture --gate exits 1, exactly 3 MISSING rows): PASS"
  else
    echo "self-test 2 (MISSING fixture --gate exits 1, exactly 3 MISSING rows): FAIL (exit $code, $n MISSING rows)"
    sed 's/^/    /' "$tmp/miss.out"
    rc=1
  fi

  # 3. AMBIGUOUS fixture: the plan nominates nothing, so every bare
  #    token hit in the shared tree is unattributable (#920) —
  #    triage state, never a credit.
  bash "${BASH_SOURCE[0]}" "$fx/plan-ambiguous.md" --gate --root "$fx/tree" \
    >"$tmp/amb.out" 2>&1 && code=0 || code=$?
  n=$(grep -c 'AMBIGUOUS (' "$tmp/amb.out" || true)
  if [[ $code -eq 1 && $n -eq 3 ]]; then
    echo "self-test 3 (AMBIGUOUS fixture --gate exits 1, exactly 3 AMBIGUOUS rows): PASS"
  else
    echo "self-test 3 (AMBIGUOUS fixture --gate exits 1, exactly 3 AMBIGUOUS rows): FAIL (exit $code, $n AMBIGUOUS rows)"
    sed 's/^/    /' "$tmp/amb.out"
    rc=1
  fi

  # 4. INVENTORY-MISS fixture (#760): wiring resolves but the callback
  #    has no SEAM_INVENTORY entry — triage state, gate-failing.
  bash "${BASH_SOURCE[0]}" "$fx/plan-inventmiss.md" --gate --root "$fx/tree" \
    >"$tmp/inv.out" 2>&1 && code=0 || code=$?
  n=$(grep -c 'INVENTORY-MISS (' "$tmp/inv.out" || true)
  if [[ $code -eq 1 && $n -eq 1 ]]; then
    echo "self-test 4 (INVENTORY-MISS fixture --gate exits 1, exactly 1 INVENTORY-MISS row): PASS"
  else
    echo "self-test 4 (INVENTORY-MISS fixture --gate exits 1, exactly 1 INVENTORY-MISS row): FAIL (exit $code, $n INVENTORY-MISS rows)"
    sed 's/^/    /' "$tmp/inv.out"
    rc=1
  fi

  # 5. Bound-test naming a nonexistent test: the mapping is a lie, so
  #    the bound row reads MISSING — a gate failure, never a pass.
  sed 's/bound-test: US-1\/S2 -> fixture_journeys.rs::us1_s2_toast_shown/bound-test: US-1\/S2 -> fixture_journeys.rs::us9_s9_does_not_exist/' \
    "$fx/plan.md" >"$tmp/plan-deadbound.md"
  bash "${BASH_SOURCE[0]}" "$tmp/plan-deadbound.md" --gate --root "$fx/tree" \
    >"$tmp/deadbound.out" 2>&1 && code=0 || code=$?
  n=$(grep -c ' journey test → MISSING$' "$tmp/deadbound.out" || true)
  b=$(grep -c ' BOUND (' "$tmp/deadbound.out" || true)
  if [[ $code -eq 1 && $n -eq 1 && $b -eq 0 ]]; then
    echo "self-test 5 (dead bound-test --gate exits 1, the bound step MISSING, 0 BOUND): PASS"
  else
    echo "self-test 5 (dead bound-test --gate exits 1, the bound step MISSING, 0 BOUND): FAIL (exit $code, $n journey MISSING, $b BOUND)"
    sed 's/^/    /' "$tmp/deadbound.out"
    rc=1
  fi

  # 6. Malformed header: hard parse error.
  sed 's/| ID | Surface file |/| Id | Surface file |/' "$fx/plan.md" \
    >"$tmp/plan-broken.md"
  bash "${BASH_SOURCE[0]}" "$tmp/plan-broken.md" --gate --root "$fx/tree" \
    >"$tmp/broken.out" 2>&1 && code=0 || code=$?
  if [[ $code -eq 2 ]]; then
    echo "self-test 6 (malformed header exits 2): PASS"
  else
    echo "self-test 6 (malformed header exits 2): FAIL (exit $code)"
    sed 's/^/    /' "$tmp/broken.out"
    rc=1
  fi

  # 7. #965 prefix-miss: a plan row naming a callback/command against a
  #    tree carrying only the suffixed spellings does NOT resolve —
  #    fixed-substring matching (pre-#965) credited both rows FOUND.
  bash "${BASH_SOURCE[0]}" "$fx/plan-prefixmiss.md" --gate --root "$fx/tree" \
    >"$tmp/pmiss.out" 2>&1 && code=0 || code=$?
  c=$(grep -c 'callback save → MISSING$' "$tmp/pmiss.out" || true)
  d=$(grep -c 'command LogHabit → MISSING$' "$tmp/pmiss.out" || true)
  if [[ $code -eq 1 && $c -eq 1 && $d -eq 1 ]]; then
    echo "self-test 7 (#965 prefix-miss --gate exits 1, the colliding-only rows MISSING): PASS"
  else
    echo "self-test 7 (#965 prefix-miss --gate exits 1, the colliding-only rows MISSING): FAIL (exit $code, $c callback, $d command)"
    sed 's/^/    /' "$tmp/pmiss.out"
    rc=1
  fi

  # 8. #965 no-miss guard: honest occurrences adjacent to the colliding
  #    spellings still resolve FOUND — the boundary anchor must not
  #    turn an honest match into a miss (round law c).
  bash "${BASH_SOURCE[0]}" "$fx/plan-prefixfound.md" --gate --root "$fx/tree" \
    >"$tmp/pfound.out" 2>&1 && code=0 || code=$?
  c=$(grep -c 'callback load → FOUND' "$tmp/pfound.out" || true)
  d=$(grep -c 'command DropHabit → FOUND' "$tmp/pfound.out" || true)
  m=$(grep -c 'MISSING$' "$tmp/pfound.out" || true)
  if [[ $code -eq 0 && $c -eq 1 && $d -eq 1 && $m -eq 0 ]]; then
    echo "self-test 8 (#965 prefix-found --gate exits 0, honest rows FOUND beside colliding spellings): PASS"
  else
    echo "self-test 8 (#965 prefix-found --gate exits 0, honest rows FOUND beside colliding spellings): FAIL (exit $code, $c callback, $d command, $m MISSING)"
    sed 's/^/    /' "$tmp/pfound.out"
    rc=1
  fi

  # 9. #967 fixture-tree exclusion: a token present ONLY inside a
  #    nested scripts/fixtures/ux-coverage-fixture/tree under the scan
  #    root (via a copied script+fixtures layout) reads MISSING —
  #    pre-#967 the fixture file satisfied the evidence universe.
  local repo7="$tmp/repo7"
  mkdir -p "$repo7/scripts/fixtures" "$repo7/tests"
  cp "${BASH_SOURCE[0]}" "$repo7/scripts/"
  cp -r "$fx" "$repo7/scripts/fixtures/"
  printf '// StateCommand variants probe (#967): this token lives only\n// inside the fixture tree, which is never evidence.\n' \
    >>"$repo7/scripts/fixtures/ux-coverage-fixture/tree/messages.rs"
  cp "$fx/tree/tests/"*.rs "$repo7/tests/"
  bash "$repo7/scripts/ux-coverage.sh" "$fx/plan-fixturepin.md" --gate --root "$repo7" \
    >"$tmp/fxpin.out" 2>&1 && code=0 || code=$?
  c=$(grep -c 'command StateCommand variants → MISSING$' "$tmp/fxpin.out" || true)
  if [[ $code -eq 1 && $c -eq 1 ]]; then
    echo "self-test 9 (#967 nested fixture tree excluded, probe row MISSING): PASS"
  else
    echo "self-test 9 (#967 nested fixture tree excluded, probe row MISSING): FAIL (exit $code, $c probe rows)"
    sed 's/^/    /' "$tmp/fxpin.out"
    rc=1
  fi

  # 10. #970 build-output exclusion: a fn present ONLY under target/
  #     does not satisfy a bound-test row (MISSING, not BOUND), while
  #     the honest bound rows stay BOUND.
  local repo8="$tmp/repo8"
  mkdir -p "$repo8/tests" "$repo8/target/debug"
  printf '#[test]\nfn us1_s2_toast_shown() {}\n\n#[test]\nfn us2_s1_badge_renders() {}\n' \
    >"$repo8/tests/fixture_journeys.rs"
  printf 'fn us1_s1_fixture_create() {}\n' >"$repo8/target/debug/out.rs"
  bash "${BASH_SOURCE[0]}" "$fx/plan-targetpin.md" --gate --root "$repo8" \
    >"$tmp/tpin.out" 2>&1 && code=0 || code=$?
  c=$(grep -c ' journey test → MISSING$' "$tmp/tpin.out" || true)
  b=$(grep -c ' BOUND (' "$tmp/tpin.out" || true)
  if [[ $code -eq 1 && $c -eq 1 && $b -eq 1 ]]; then
    echo "self-test 10 (#970 target/ excluded, target-only bound row MISSING, honest row BOUND): PASS"
  else
    echo "self-test 10 (#970 target/ excluded, target-only bound row MISSING, honest row BOUND): FAIL (exit $code, $c journey MISSING, $b BOUND)"
    sed 's/^/    /' "$tmp/tpin.out"
    rc=1
  fi

  # 11. #1073 tier determinism: the token lives in BOTH the production
  #     and the test-support messages twin; the command citation must
  #     be the production file, identically across 3 runs (the old bug
  #     was find-order luck — the assertion is stability itself).
  local cmd_ev run_i
  local -a runs=()
  for run_i in 1 2 3; do
    bash "${BASH_SOURCE[0]}" "$fx/plan-tiersort.md" --gate --root "$fx/tree" \
      >"$tmp/tiersort-$run_i.out" 2>&1 && code=0 || code=$?
    cmd_ev=$(grep -oE 'command TierProbe → FOUND \([^)]*\)' "$tmp/tiersort-$run_i.out" || true)
    runs+=("$cmd_ev")
    if [[ $code -ne 0 || $cmd_ev != *"/src/messages.rs:"* ]]; then
      echo "self-test 11 (#1073 production-tier citation, run $run_i): FAIL (exit $code, evidence: ${cmd_ev:-none})"
      sed 's/^/    /' "$tmp/tiersort-$run_i.out"
      rc=1
    fi
  done
  if [[ ${runs[0]} == "${runs[1]}" && "${runs[1]}" == "${runs[2]}" && -n ${runs[0]:-} ]]; then
    [[ $rc -eq 0 ]] && echo "self-test 11 (#1073 production-tier citation stable across 3 runs): PASS"
  elif [[ $rc -eq 0 ]]; then
    echo "self-test 11 (#1073 production-tier citation stable across 3 runs): FAIL (runs differ: ${runs[*]})"
    rc=1
  fi

  # 12. #1076 bound-test grep excludes the fixture tree: a nomination
  #     matching fn us1_s1_fixture_create — present ONLY inside a
  #     copied scripts/fixtures/ux-coverage-fixture under the scan root
  #     — reads MISSING (pre-#1076 it read BOUND out of the fixture),
  #     while the honest bound row stays BOUND.
  local repo11="$tmp/repo11"
  mkdir -p "$repo11/scripts/fixtures" "$repo11/tests"
  cp "${BASH_SOURCE[0]}" "$repo11/scripts/"
  cp -r "$fx" "$repo11/scripts/fixtures/"
  printf '#[test]\nfn us1_s2_toast_shown() {}\n\n#[test]\nfn us2_s1_badge_renders() {}\n' \
    >"$repo11/tests/fixture_journeys.rs"
  bash "$repo11/scripts/ux-coverage.sh" "$fx/plan-targetpin.md" --gate --root "$repo11" \
    >"$tmp/fxbound.out" 2>&1 && code=0 || code=$?
  c=$(grep -c ' journey test → MISSING$' "$tmp/fxbound.out" || true)
  b=$(grep -c ' BOUND (' "$tmp/fxbound.out" || true)
  if [[ $code -eq 1 && $c -eq 1 && $b -eq 1 ]]; then
    echo "self-test 12 (#1076 fixture-only bound row MISSING, honest row BOUND): PASS"
  else
    echo "self-test 12 (#1076 fixture-only bound row MISSING, honest row BOUND): FAIL (exit $code, $c journey MISSING, $b BOUND)"
    sed 's/^/    /' "$tmp/fxbound.out"
    rc=1
  fi

  # 13. #1073 symlink-spelled invocation: when the script and the scan
  #     root are addressed through a symlink, dirname keeps the invoked
  #     spelling and the fixture prune silently never matched (the
  #     tiered sort then ranked the fixture's seam_inventory over the
  #     real one, flipping resolved wiring rows to INVENTORY-MISS).
  #     The fixture-only probe row must read MISSING through symlinks,
  #     exactly as self-test 9 does through plain paths.
  local repo13="$tmp/repo13"
  mkdir -p "$repo13/scripts/fixtures" "$repo13/tests"
  cp "${BASH_SOURCE[0]}" "$repo13/scripts/"
  cp -r "$fx" "$repo13/scripts/fixtures/"
  printf '// StateCommand variants probe (#967/#1073): this token lives only\n// inside the fixture tree, which is never evidence.\n' \
    >>"$repo13/scripts/fixtures/ux-coverage-fixture/tree/messages.rs"
  cp "$fx/tree/tests/"*.rs "$repo13/tests/"
  ln -s "$repo13" "$tmp/repo13-link"
  bash "$tmp/repo13-link/scripts/ux-coverage.sh" "$fx/plan-fixturepin.md" --gate --root "$tmp/repo13-link" \
    >"$tmp/symlink.out" 2>&1 && code=0 || code=$?
  c=$(grep -c 'command StateCommand variants → MISSING$' "$tmp/symlink.out" || true)
  i=$(grep -c 'INVENTORY-MISS (' "$tmp/symlink.out" || true)
  if [[ $code -eq 1 && $c -eq 1 && $i -eq 0 ]]; then
    echo "self-test 13 (#1073 symlink-spelled invocation still prunes the fixture tree): PASS"
  else
    echo "self-test 13 (#1073 symlink-spelled invocation still prunes the fixture tree): FAIL (exit $code, $c probe MISSING, $i INVENTORY-MISS)"
    sed 's/^/    /' "$tmp/symlink.out"
    rc=1
  fi

  # 14. #1181 inline code spans in the §6 wiring row: the ActionId /
  #     Commands cells wrap prefixed tokens in backticks. Pre-#1181
  #     the spans defeated token parsing (every token MISSING);
  #     post-#1181 cellget strips the spans and the row resolves
  #     exactly like the bare plan.md equivalent (exit 0, 1 BOUND).
  bash "${BASH_SOURCE[0]}" "$fx/plan-backtickprefix.md" --gate --root "$fx/tree" \
    >"$tmp/btprefix.out" 2>&1 && code=0 || code=$?
  n=$(grep -c ' BOUND (' "$tmp/btprefix.out" || true)
  m=$(grep -c 'MISSING$' "$tmp/btprefix.out" || true)
  if [[ $code -eq 0 && $n -eq 1 && $m -eq 0 ]]; then
    echo "self-test 14 (#1181 backticked prefixed wiring tokens resolve, exit 0): PASS"
  else
    echo "self-test 14 (#1181 backticked prefixed wiring tokens resolve, exit 0): FAIL (exit $code, $n BOUND, $m MISSING)"
    sed 's/^/    /' "$tmp/btprefix.out"
    rc=1
  fi

  # 15. #1181 inline code spans in §4/§6/§8: backticked Step tokens,
  #     Dispatches cells, journey-step / Test nominations and a
  #     backticked bound-test step id must resolve exactly like the
  #     bare plan.md equivalent (exit 0, 1 BOUND).
  bash "${BASH_SOURCE[0]}" "$fx/plan-backtickstep.md" --gate --root "$fx/tree" \
    >"$tmp/btstep.out" 2>&1 && code=0 || code=$?
  n=$(grep -c ' BOUND (' "$tmp/btstep.out" || true)
  m=$(grep -c 'MISSING$' "$tmp/btstep.out" || true)
  if [[ $code -eq 0 && $n -eq 1 && $m -eq 0 ]]; then
    echo "self-test 15 (#1181 backticked step/bound-test tokens resolve, exit 0): PASS"
  else
    echo "self-test 15 (#1181 backticked step/bound-test tokens resolve, exit 0): FAIL (exit $code, $n BOUND, $m MISSING)"
    sed 's/^/    /' "$tmp/btstep.out"
    rc=1
  fi

  rm -rf "$tmp"
  return $rc
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
  local plan="" gate=0 root="" selftest=0
  while [[ $# -gt 0 ]]; do
    case $1 in
      --gate) gate=1 ;;
      --root) [[ $# -ge 2 ]] || { usage; exit 64; }; root=$2; shift ;;
      --self-test) selftest=1 ;;
      -h|--help) usage; exit 0 ;;
      -*) echo "ux-coverage: unknown option: $1" >&2; usage; exit 64 ;;
      *) if [[ -n $plan ]]; then
           echo "ux-coverage: two plan files given" >&2; usage; exit 64
         fi
         plan=$1 ;;
    esac
    shift
  done

  if [[ $selftest -eq 1 ]]; then
    self_test
    return $?
  fi

  if [[ -z $plan || ! -f $plan ]]; then
    echo "ux-coverage: plan file missing or not a file: ${plan:-<none>}" >&2
    usage
    exit 64
  fi

  if [[ -z $root ]]; then
    root="$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)"
  fi

  local records=""
  records=$(parse_plan "$plan") || { echo "$records" >&2; exit 2; }

  # Evidence file universe (find by name so --root trees of any shape
  # work), through the shared #967/#970 predicate: the self-test
  # fixture tree and build output are never evidence. The fixture
  # prune is skipped when the scan root itself is the fixture tree or
  # inside it (--root is the self-test's tree) — the exclusion is
  # evidence-scoped, not a kill switch for explicit roots.
  local EVID_ROOT
  local FX_ROOT=""
  local script_dir
  # (#1073) realpath both roots: dirname keeps the INVOKED spelling —
  # when the script (or --root) is addressed through a symlink, find
  # walks the resolved path and a literal `-path "$FX_ROOT" -prune`
  # never matches, silently un-pruning the fixture tree (the tiered
  # sort then ranks the fixture's seam_inventory over the real one).
  # `pwd -P` resolves; the evidence-scoped prune actually prunes.
  # (#1186 follow-up) EVID_ROOT resolves too — it was captured from
  # the INVOKED spelling before the `pwd -P` pass, so a symlinked
  # --root left find's walk spelling and the prune spelling in two
  # trees: the fixture file survived the prune, its tier0 rank (no
  # /tests/ segment) won `head -n 1`, and every #760 inventory
  # cross-check read the fixture's seam_inventory (INVENTORY-MISS).
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
  root=$(cd "$root" && pwd -P)
  EVID_ROOT=$root
  if [[ -d "$script_dir/fixtures/ux-coverage-fixture" ]] \
     && ! [[ "$root" -ef "$script_dir/fixtures/ux-coverage-fixture" ]] \
     && [[ "$root/" != "$script_dir/fixtures/ux-coverage-fixture/"* ]]; then
    FX_ROOT="$script_dir/fixtures/ux-coverage-fixture"
  fi
  local -a slint_files=() adapter_files=() messages_files=() \
        requests_files=() actions_files=() test_files=()
  mapfile -t slint_files < <(find_evidence '*.slint')
  mapfile -t adapter_files < <(find_evidence 'adapter.rs')
  mapfile -t messages_files < <(find_evidence 'messages.rs')
  mapfile -t requests_files < <(find_evidence 'requests.rs')
  mapfile -t actions_files < <(find_evidence 'actions.rs')
  # (#1074) the former all_rs_files universe was never consumed — the
  # step-4 unscoped fallback scans test_files only by design (#920:
  # ambient hits are AMBIGUOUS, never a credit).

  local test_root
  test_root=$(grep -m1 '^Test-root:' "$plan" | sed 's/^Test-root:[[:space:]]*//' || true)
  if [[ -z $test_root ]]; then
    test_root="crates/taskboard-ui-slint/tests"
  fi
  mapfile -t test_files < <(find "${root}/${test_root}" -type f -name '*.rs' 2>/dev/null)

  # seam_inventory.rs (the adapter's structural manifest) — located once
  # by the same name-based universe. Absence (e.g. the miniature fixture
  # tree) skips the #760 cross-check for that run; it is a fixture-shape
  # fact, not a finding.
  local inv_file=""
  inv_file=$(find_evidence 'seam_inventory.rs' | head -n 1)

  # Parse records into parallel per-table arrays.
  local -a cap_rows=() rul_rows=() jrn_stories=() jrn_steps=() jrn_rows=() \
        wir_rows=() del_rows=()
  local rec kind extra rest cells
  while IFS= read -r rec; do
    [[ -n $rec ]] || continue
    kind=${rec#ROW }
    kind=${kind%% *}
    rest=${rec#ROW "$kind" }
    extra=${rest%% *}
    cells=${rest#* }
    case $kind in
      cap) cap_rows+=("$cells") ;;
      rul) rul_rows+=("$cells") ;;
      jrn) jrn_stories+=("$extra"); jrn_rows+=("$cells") ;;
      wir) wir_rows+=("$cells") ;;
      del) del_rows+=("$cells") ;;
    esac
  done <<<"$records"

  # Command-contract delta: name -> Kind (selects the evidence file).
  local -A delta_kind=()
  local IFS d_kind d_name
  for cells in "${del_rows[@]}"; do
    IFS=$'\x1f' read -r d_kind d_name _ _ <<<"$cells"
    if [[ -n $d_kind && -n $d_name ]]; then
      delta_kind[$d_name]=$d_kind
    fi
  done

  # --- Step nominations (#920, plan §3 D1). The plan nominates its own
  #     evidence in the §6 Test column; the gate trusts nothing ambient.
  #     exact_nom:  fn-name -> comma-list of nominated file sides
  #     scoped_nom: bare us<n>_s<k> token -> nominated file side
  local -A exact_nom=() scoped_nom=()
  local w_test nom_side nom_name
  for cells in "${wir_rows[@]}"; do
    IFS=$'\x1f' read -r _ _ _ _ _ _ _ _ _ w_test <<<"$cells"
    while IFS= read -r nom_pair; do
      [[ -n $nom_pair ]] || continue
      nom_side=${nom_pair%%::*}
      nom_name=${nom_pair##*::}
      if is_bare_token "$nom_name"; then
        scoped_nom[$nom_name]=$nom_side
      else
        exact_nom[$nom_name]="${exact_nom[$nom_name]:-}${exact_nom[$nom_name]:+,}${nom_side}"
      fi
    done < <(grep -oE '[A-Za-z0-9_/.-]+\.rs::[A-Za-z0-9_]+' <<<"${w_test:-}" || true)
  done

  # --- Bound-test mapping (#768, plan §3 D4). Retro plans carry
  #     `bound-test: US-<n>/S<k> -> <file>.rs::<fn_name>` lines anywhere
  #     in the body. A line naming a step the plan does not declare in
  #     §4 is a hard parse error — fail loudly, never silently ignore.
  local -A bound_nom=() declared_steps=()
  local idx step step_id
  for idx in "${!jrn_rows[@]}"; do
    IFS=$'\x1f' read -r step _ _ _ _ _ <<<"${jrn_rows[$idx]}"
    # canonical form US-<n>/S<k>; accept the D4 grammar's `S-<k>` by
    # normalizing only the step-half hyphen (never the US- one)
    step_id=${step/\/S-/\/S}
    declared_steps[$step_id]=1
  # shellcheck disable=SC2004 # idx is a plain integer loop index; the $ keeps the subscript unambiguous
  jrn_steps[$idx]=$step
  done
  local b_line b_step b_name
  while IFS= read -r b_line; do
    # shellcheck disable=SC2001 # trimming every leading-space char class; a ${var//} replacement cannot express a class
    b_line=$(sed 's/^[[:space:]]*//' <<<"$b_line")
    [[ $b_line =~ ^bound-test:[[:space:]]*(\`?US-[0-9]+/S-?[0-9]+\`?)[[:space:]]*-\>[[:space:]]*([A-Za-z0-9_/.-]+\.rs)::([A-Za-z0-9_]+)[[:space:]]*$ ]] || {
      echo "PARSE-ERROR: bound-tests: malformed line: $b_line" >&2
      exit 2
    }
    b_step=${BASH_REMATCH[1]}
    b_name=${BASH_REMATCH[3]}
    b_step=${b_step//\`/} # (#1181) an inline code span around the step id
    b_step=${b_step/\/S-/\/S}
    if [[ -z ${declared_steps[$b_step]:-} ]]; then
      echo "PARSE-ERROR: bound-tests: unknown step id: ${BASH_REMATCH[1]}" >&2
      exit 2
    fi
    bound_nom[$b_step]=$b_name
  # (leading indentation tolerated — plans list mappings under a
  # bullet in §8; the grammar binds the line shape, not its margin)
  done < <(grep -E '^[[:space:]]*bound-test:' "$plan" || true)

  # Resolve a nomination file side to a path: sides containing `/` are
  # root-relative; bare names live under the Test-root, falling back to
  # a by-name search (src unit-test files like lib.rs).
  nom_file() { # nom_file <file-side> -> path
    local f=$1
    if [[ $f == */* ]]; then
      echo "${root}/${f}"
    elif [[ -f "${root}/${test_root}/${f}" ]]; then
      echo "${root}/${test_root}/${f}"
    else
      find_evidence "$f" | head -n 1
    fi
  }

  # --- Journey-step resolution (plan §3 D2). Priority: exact-name
  #     nomination > file-scoped token nomination > bound-test >
  #     unscoped (ambient hits are AMBIGUOUS, never a credit).
  #     Sets: verdict_ok=1 when the step is covered (FOUND/BOUND).
  resolve_step() { # resolve_step <label> <step-id> <token>
    local label=$1 sid=$2 token=$3
    local name files f path
    local -a hits=()

    # 1. Exact-name nomination covering the token.
    for name in "${!exact_nom[@]}"; do
      [[ $name == "$token" || $name == "${token}_"* ]] || continue
      IFS=',' read -ra files <<<"${exact_nom[$name]}"
      for f in "${files[@]}"; do
        path=$(nom_file "$f")
        [[ -n $path && -f $path ]] || continue
        mapfile -t hits < <(fn_boundary_matches "$name" "$path")
        if (( ${#hits[@]} > 0 )); then
          FOUND_EV=$(ev_fileline "${hits[0]}")
          emit_found "$label"
          return 0
        fi
      done
      # Nominated, but the named test does not exist — a lie, not a pass.
      emit_missing "$label"
      return 1
    done

    # 2. File-scoped token nomination.
    if [[ -n ${scoped_nom[$token]:-} ]]; then
      path=$(nom_file "${scoped_nom[$token]}")
      if [[ -n $path && -f $path ]]; then
        mapfile -t hits < <(fn_boundary_matches "$token" "$path")
      else
        hits=()
      fi
      case ${#hits[@]} in
        0) emit_missing "$label"; return 1 ;;
        1) FOUND_EV=$(ev_fileline "${hits[0]}"); emit_found "$label"; return 0 ;;
        *) emit_ambiguous "$label" "${hits[@]}"; return 1 ;;
      esac
    fi

    # 3. Bound-test mapping (retro cohort).
    if [[ -n ${bound_nom[$sid]:-} ]]; then
      name=${bound_nom[$sid]}
      # (#1076) mirror find_evidence's evidence-scoped fixture prune:
      # when the script-relative fixture tree exists and the scan root
      # is not inside it (FX_ROOT is set exactly then), exclude the
      # fixture root by its basename (grep matches --exclude-dir per
      # path level, so a slash-bearing path would never match) — a
      # bound-test nomination must not read BOUND out of the miniature
      # fixture tree. When the root IS the fixture tree (self-tests
      # 1/5/8 posture) FX_ROOT is unset and the grep is unchanged.
      local -a grep_args=(grep -rnE --include='*.rs'
                          --exclude-dir=target --exclude-dir=node_modules
                          --exclude-dir=.git)
      if [[ -n ${FX_ROOT:-} ]]; then
        grep_args+=(--exclude-dir="$(basename "$FX_ROOT")")
      fi
      mapfile -t hits < <("${grep_args[@]}" "fn ${name}(_|\\()" "$root" 2>/dev/null | head -n 1 || true)
      if (( ${#hits[@]} > 0 )); then
        FOUND_EV=$(ev_fileline "${hits[0]}")
        emit_bound "$label"
        return 0
      fi
      emit_missing "$label"
      return 1
    fi

    # 4. Unscoped: any hit in the shared tree is unattributable — that
    #     attribution failure *is* #920.
    mapfile -t hits < <(fn_boundary_matches "$token" "${test_files[@]}")
    if (( ${#hits[@]} == 0 )); then
      emit_missing "$label"
      return 1
    fi
    emit_ambiguous "$label" "${hits[@]}"
    return 1
  }

  # --- Journey-step checks first (capability cells compose their results).
  local -A story_exists=() story_bad=()
  local idx story step token
  for idx in "${!jrn_rows[@]}"; do
    step=${jrn_steps[$idx]}
    story=${jrn_stories[$idx]}
    story_exists[$story]=1
    token=$(tr 'A-Z/' 'a-z_' <<<"$step" | tr -d '-')
    if resolve_step "${step} journey test" "$step" "$token"; then
      story_bad[$story]=${story_bad[$story]:-0}
    else
      story_bad[$story]=1
    fi
  done

  # --- Ruling checks: reason must be non-empty.
  local r_id r_reason
  for cells in "${rul_rows[@]}"; do
    IFS=$'\x1f' read -r r_id _ r_reason _ <<<"$cells"
    if [[ -n $r_id ]] && ! is_absent "$r_reason"; then
      emit_found_in_plan "${r_id} reason" "plan §2 rulings"
    else
      emit_missing "${r_id} reason"
    fi
  done

  # --- Capability-matrix cells: every US-n citation must resolve to a
  #     story whose every step is covered (AMBIGUOUS poisons the
  #     citation — never a silent credit).
  local capability cell ref
  for cells in "${cap_rows[@]}"; do
    IFS=$'\x1f' read -r capability _ _ _ _ _ _ _ <<<"$cells"
    for cell in ${cells//$'\x1f'/ }; do
      while IFS= read -r ref; do
        [[ -n $ref ]] || continue
        if [[ ${story_exists[$ref]:-0} -eq 1 && ${story_bad[$ref]:-0} -eq 0 ]]; then
          emit_found_in_plan "${capability} cites ${ref}" "story steps covered"
        else
          emit_missing "${capability} cites ${ref}"
        fi
      done < <(grep -oE 'US-[0-9]+' <<<"$cell" | sort -u)
    done
  done

  # --- Wiring-inventory checks.
  local w_id w_cb w_action w_cmds w_steps tok kind
  local slint_ev adapter_ev
  local -a files_sel=()
  for cells in "${wir_rows[@]}"; do
    IFS=$'\x1f' read -r w_id _ _ w_cb _ w_action w_cmds _ w_steps _ <<<"$cells"
    [[ -n $w_id ]] || continue

    if ! is_absent "$w_cb"; then
      if tok_match "$w_cb" "${slint_files[@]}"; then
        slint_ev=$FOUND_EV
        if adapter_match "$w_cb" "${adapter_files[@]}"; then
          adapter_ev=$FOUND_EV
          # #760 cross-check: a resolved callback must be listed in the
          # adapter's structural manifest. Callbacks that already MISS
          # keep MISSING — this is an additional honesty pass on
          # resolved wiring, not a substitute.
          if [[ -n $inv_file ]] \
             && ! grep -qF -- "\"${w_cb}\"" "$inv_file"; then
            emit_inventory_miss "${w_id} callback ${w_cb}" "$slint_ev" "$adapter_ev"
          else
            FOUND_EV=$slint_ev
            emit_found "${w_id} callback ${w_cb}"
          fi
        else
          emit_missing "${w_id} callback ${w_cb}"
        fi
      else
        emit_missing "${w_id} callback ${w_cb}"
      fi
    fi

    if ! is_absent "$w_action"; then
      tok=${w_action#ActionId::}
      if tok_match "$tok" "${actions_files[@]}"; then
        emit_found "${w_id} action ${tok}"
      else
        emit_missing "${w_id} action ${tok}"
      fi
    fi

    local any_cmd=0
    while IFS= read -r tok; do
      if is_absent "$tok"; then
        continue
      fi
      any_cmd=1
      kind=""
      if [[ $tok == StateCommand::* ]]; then
        kind=StateCommand; tok=${tok#StateCommand::}
      elif [[ $tok == UiRequest::* ]]; then
        kind=UiRequest; tok=${tok#UiRequest::}
      elif [[ $tok == ActionId::* ]]; then
        kind=ActionId; tok=${tok#ActionId::}
      else
        kind=${delta_kind[$tok]:-}
      fi
      case $kind in
        StateCommand) files_sel=("${messages_files[@]}") ;;
        UiRequest) files_sel=("${requests_files[@]}") ;;
        ActionId) files_sel=("${actions_files[@]}") ;;
        *) files_sel=("${messages_files[@]}" "${requests_files[@]}" "${actions_files[@]}") ;;
      esac
      if tok_match "$tok" "${files_sel[@]}"; then
        emit_found "${w_id} command ${tok}"
      else
        emit_missing "${w_id} command ${tok}"
      fi
    done < <(tr ';' '\n' <<<"$w_cmds" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

    if [[ $any_cmd -eq 0 ]]; then
      # A row with no command variant must say why in Journey steps.
      if is_absent "$w_steps"; then
        emit_missing "${w_id} no-commands reason"
      else
        emit_found_in_plan "${w_id} no-commands reason" "plan §6"
      fi
    fi
  done

  echo "---"
  echo "checks: $((n_found + n_missing + n_ambiguous + n_bound + n_invmiss)), FOUND: ${n_found}, MISSING: ${n_missing}, AMBIGUOUS: ${n_ambiguous}, BOUND: ${n_bound}, INVENTORY-MISS: ${n_invmiss}"
  if [[ $gate -eq 1 && $((n_missing + n_ambiguous + n_invmiss)) -gt 0 ]]; then
    return 1
  fi
  return 0
}

main "$@"
