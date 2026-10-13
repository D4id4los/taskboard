#!/usr/bin/env bash
# Quarantine corrupt .profraw files before the llvm-cov profile merge.
#
# Mechanism (coverage-flake investigation, 2026-10-13): every process the
# coverage build spawns (daemons, CLI clients) is itself instrumented and
# writes a profile file. Tests legitimately kill such children mid-flight
# (Child::drop SIGKILL, lock contenders, timeout expiry); a SIGKILL landing
# inside a process's profile-write window leaves a partially written file
# whose header is invalid. llvm-profdata's `-f` list merge aborts the ENTIRE
# merge on a single invalid input, so one garbage file kills an otherwise
# green coverage job. A quarantined file holds only the torn tail of a
# killed process's counters — there is no usable coverage in it to lose.
#
# Behavior: merge the profraw list once. Healthy runs pay one extra merge
# pass and stop there. If the merge fails, every file llvm-profdata warns
# about is moved into <dir>/quarantine/ (outside the merge glob) with a
# logged name — the coverage loss is deliberate and visible — and the
# merge of the remainder is retried once. Still failing after that is real
# breakage and fails the script.
#
# Usage: coverage-quarantine-profraw.sh <profraw-dir>
set -euo pipefail

dir="${1:?usage: coverage-quarantine-profraw.sh <profraw-dir>}"
shopt -s nullglob
files=("$dir"/*.profraw)
if [ "${#files[@]}" -eq 0 ]; then
    echo "coverage-quarantine: no profraw files in $dir, nothing to do"
    exit 0
fi

profdata="$(find "$(rustc --print sysroot)/lib/rustlib" \
    -name llvm-profdata -type f | head -n 1)"
if [ -z "$profdata" ]; then
    echo "coverage-quarantine: llvm-profdata not found under $(rustc --print sysroot)" >&2
    exit 1
fi

list="$dir/taskboard-profraw-list"
: > "$list"
printf '%s\n' "${files[@]}" > "$list"

try_merge() {
    "$profdata" merge -sparse -f "$list" -o "$dir/quarantine-probe.profdata"
}

if try_merge; then
    rm -f "$dir/quarantine-probe.profdata"
    echo "coverage-quarantine: all ${#files[@]} profraw files valid"
    exit 0
fi

mkdir -p "$dir/quarantine"
# Extract the paths llvm-profdata rejected (one `warning: <path>: ...`
# line per invalid file) and move them out of the merge glob.
while IFS= read -r bad; do
    [ -f "$bad" ] || continue
    echo "coverage-quarantine: quarantining corrupt profraw (coverage loss, killed-writer artifact): $bad"
    mv "$bad" "$dir/quarantine/"
done < <(try_merge 2>&1 >/dev/null | sed -n 's/^warning: \(.*\): .*/\1/p' | sort -u)

shopt -u nullglob
remaining=("$dir"/*.profraw)
if [ "${#remaining[@]}" -eq 0 ]; then
    echo "coverage-quarantine: every profraw was quarantined; nothing left to merge" >&2
    exit 1
fi

: > "$list"
printf '%s\n' "${remaining[@]}" > "$list"

if ! try_merge; then
    echo "coverage-quarantine: merge still failing after quarantine" >&2
    exit 1
fi
rm -f "$dir/quarantine-probe.profdata"
echo "coverage-quarantine: merge succeeded after quarantining corrupt files"
