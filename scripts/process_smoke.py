#!/usr/bin/env python3
"""Process smoke harnesses for the kiosk budget rows (audit S1/S3).

Two local, manual-only measurements against the real release binary and
a fresh bench-fixture database (N = 10 000 tasks, built on demand):

  rss          spawn `taskboard daemon` standalone, poll readiness via
               `taskboard status --json` (daemon == true), soak /proc
               VmRSS to a bounded plateau, report steady/peak kB.
  cold-start   spawn `taskboard gui` and time spawn -> the structured
               `moment = "first-frame"` tracing event on the child's
               JSON log stream (TASKBOARD_LOG_FORMAT=json is set for
               the child; the field, never message text, is selected).

Both are smoke REFERENCES, never gates: nothing in CI runs them and no
exit code encodes a budget comparison — the kiosk bring-up spike owns
device-class verdicts. Linux-only (reads /proc/<pid>/status).

Usage:
  process_smoke.py rss [options]
  process_smoke.py cold-start [options]
  process_smoke.py --help

Common options (both subcommands):
  --tasks <N>                fixture size, default 10000
  --fixture <PATH>           reuse an existing database file (default:
                             build a fresh one per run into the scratch
                             dir); missing file is a precondition error
  --taskboard <PATH>         the taskboard binary (default
                             target/release/taskboard; missing binaries
                             are built via cargo unless --no-build)
  --no-build                 never invoke cargo; a missing binary is a
                             precondition error
  --shutdown-grace-secs <S>  graceful-shutdown wait before SIGKILL,
                             default 20
  --keep-dir                 keep the scratch dir on success (it is
                             always kept on failure)

rss-only options:
  --ready-deadline-secs <S>  readiness poll deadline, default 30
  --soak-deadline-secs <S>   RSS-soak deadline, default 120
  --soak-min-secs <S>        minimum soak before plateau may be
                             declared, default 2
  --plateau-samples <N>      sliding-window length for the plateau
                             predicate, default 8
  --plateau-band-kb <KB>     max spread (max - min) inside the window,
                             default 4096

cold-start-only options:
  --deadline-secs <S>        first-frame event deadline, default 60

Exit codes: 0 measured and reported; 2 usage (argparse); 3 measurement
failure (deadline expiry, no plateau, missing first-frame event — the
captured child logs are printed); 4 precondition failure (non-Linux
host, no display for cold-start, missing binary under --no-build, a
failed cargo build, missing --fixture path).
"""

import argparse
import json
import shutil
import os
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUDGET_REFERENCE_MB = 200
BUDGET_REFERENCE_S = 5

READY_POLL_SECS = 0.2
RSS_SAMPLE_SECS = 0.25

EXIT_MEASUREMENT = 3
EXIT_PRECONDITION = 4


class PreconditionError(Exception):
    """A typed precondition failure (exit 4)."""


class MeasurementError(Exception):
    """A typed measurement failure (exit 3)."""


def announce(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


# ------------------------------------------------------------------
# Binary resolution & fixture
# ------------------------------------------------------------------


def resolve_binary(path: Path, no_build: bool) -> Path:
    if path.is_file():
        return path
    if no_build:
        raise PreconditionError(f"binary missing and --no-build given: {path}")
    announce("building release binary (first run can take minutes): cargo build --release -p taskboard-app")
    subprocess.run(
        ["cargo", "build", "--release", "-p", "taskboard-app"],
        cwd=ROOT,
        check=True,
    )
    if not path.is_file():
        raise PreconditionError(f"cargo succeeded but the binary is missing: {path}")
    return path


def resolve_example(path: Path, no_build: bool) -> Path:
    if path.is_file():
        return path
    if no_build:
        raise PreconditionError(f"example missing and --no-build given: {path}")
    announce(
        "building fixture example (first run can take minutes): "
        "cargo build --release -p taskboard-storage-sqlite --example build-bench-fixture"
    )
    subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "-p",
            "taskboard-storage-sqlite",
            "--example",
            "build-bench-fixture",
        ],
        cwd=ROOT,
        check=True,
    )
    if not path.is_file():
        raise PreconditionError(f"cargo succeeded but the example is missing: {path}")
    return path


def build_fixture(tasks: int, scratch: Path, args) -> Path:
    if args.fixture:
        fixture = Path(args.fixture).resolve()
        if not fixture.is_file():
            raise PreconditionError(f"--fixture path missing: {fixture}")
        announce(f"reusing fixture: {fixture}")
        return fixture
    example = resolve_example(
        ROOT / "target/release/examples/build-bench-fixture", args.no_build
    )
    fixture = scratch / "taskboard.db"
    announce(f"building fixture: N={tasks} -> {fixture}")
    subprocess.run(
        [str(example), "--out", str(fixture), "--tasks", str(tasks)],
        check=True,
    )
    return fixture


def write_config(scratch: Path, fixture: Path, desktop_mode: bool) -> Path:
    config = scratch / "taskboard.toml"
    mode = '\n[app]\nmode = "desktop"\n' if desktop_mode else ""
    config.write_text(f'[storage]\ndb_path = "{fixture}"\n{mode}')
    return config


def child_env() -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("TASKBOARD_")}
    env["TASKBOARD_LOG_FORMAT"] = "json"
    return env


# ------------------------------------------------------------------
# Process helpers
# ------------------------------------------------------------------


def spawn(binary: Path, argv: list[str], scratch: Path) -> subprocess.Popen:
    stdout = (scratch / "child-stdout.log").open("wb")
    stderr = (scratch / "child-stderr.log").open("wb")
    return subprocess.Popen(
        [str(binary), *argv],
        stdin=subprocess.DEVNULL,
        stdout=stdout,
        stderr=stderr,
        env=child_env(),
        cwd=scratch,
    )


def log_tail(scratch: Path, lines: int = 40) -> str:
    path = scratch / "child-stderr.log"
    if not path.is_file():
        return "(no child log)"
    content = path.read_bytes().decode(errors="replace").splitlines()
    tail = content[-lines:]
    return "\n".join(tail) if tail else "(child log empty)"


def terminate(child: subprocess.Popen, grace_secs: int) -> bool:
    """SIGTERM, wait the grace budget, SIGKILL fallback. Returns True
    when the process exited within the grace budget."""
    if child.poll() is not None:
        return True
    child.send_signal(signal.SIGTERM)
    try:
        child.wait(timeout=grace_secs)
        return True
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
        return False


def read_vmrss_kb(pid: int) -> int:
    """The one kernel pseudo-file the harness parses: the `VmRSS:\t<kB>`
    line of /proc/<pid>/status (key + kB value, documented format)."""
    with open(f"/proc/{pid}/status") as status:
        for line in status:
            if line.startswith("VmRSS:"):
                return int(line.split()[1])
    raise MeasurementError(f"no VmRSS line in /proc/{pid}/status")


class Scratch:
    """A temp dir kept on failure (post-mortem) and, with --keep-dir,
    on success too; removed otherwise."""

    def __init__(self, prefix: str, keep_on_success: bool):
        self.path = Path(tempfile.mkdtemp(prefix=prefix))
        self.keep_on_success = keep_on_success
        self.failed = False

    def __enter__(self) -> Path:
        return self.path

    def __exit__(self, exc_type, _exc, _tb) -> None:
        if exc_type is not None:
            self.failed = True
        if self.failed or self.keep_on_success:
            announce(f"scratch dir kept: {self.path}")
        else:
            shutil.rmtree(self.path)


def emit_summary(summary: dict) -> None:
    print(json.dumps(summary))


# ------------------------------------------------------------------
# S1 — RSS
# ------------------------------------------------------------------


def await_ready(
    binary: Path, config: Path, child: subprocess.Popen, deadline_secs: int, scratch: Path
) -> float:
    start = time.monotonic()
    while time.monotonic() - start < deadline_secs:
        if child.poll() is not None:
            raise MeasurementError(
                f"daemon exited early (code {child.returncode}):\n{log_tail(scratch)}"
            )
        probe = subprocess.run(
            [str(binary), "--config", str(config), "--json", "status"],
            capture_output=True,
            text=True,
        )
        try:
            view = json.loads(probe.stdout)
        except json.JSONDecodeError:
            view = None
        if view is not None and view.get("daemon") is True:
            if view.get("pid") != child.pid:
                raise MeasurementError(
                    f"status pid {view.get('pid')} != child pid {child.pid}"
                )
            return time.monotonic() - start
        time.sleep(READY_POLL_SECS)
    raise MeasurementError(
        f"readiness deadline ({deadline_secs}s) expired:\n{log_tail(scratch)}"
    )


def soak_rss(child: subprocess.Popen, args) -> tuple[int, int, int, float]:
    """Samples VmRSS until a plateau (sliding window of
    `plateau_samples` consecutive samples with spread <=
    `plateau_band_kb`) after at least `soak_min_secs`. Returns
    (steady_kb, peak_kb, sample_count, soak_secs)."""
    start = time.monotonic()
    samples: list[int] = []
    while time.monotonic() - start < args.soak_deadline_secs:
        samples.append(read_vmrss_kb(child.pid))
        elapsed = time.monotonic() - start
        if len(samples) >= args.plateau_samples and elapsed >= args.soak_min_secs:
            window = samples[-args.plateau_samples :]
            if max(window) - min(window) <= args.plateau_band_kb:
                steady = sorted(window)[len(window) // 2]
                return steady, max(samples), len(samples), elapsed
        time.sleep(RSS_SAMPLE_SECS)
    raise MeasurementError(
        f"no RSS plateau within {args.soak_deadline_secs}s "
        f"({len(samples)} samples, spread of the last window "
        f"{max(samples[-args.plateau_samples:]) - min(samples[-args.plateau_samples:]) if samples else 0} kB)"
    )


def run_rss(args) -> dict:
    binary = resolve_binary(args.taskboard, args.no_build)
    with Scratch("taskboard-rss-", args.keep_dir) as scratch:
        fixture = build_fixture(args.tasks, scratch, args)
        config = write_config(scratch, fixture, desktop_mode=False)
        child = spawn(binary, ["daemon", "--config", str(config)], scratch)
        try:
            ready_secs = await_ready(
                binary, config, child, args.ready_deadline_secs, scratch
            )
            steady_kb, peak_kb, count, soak_secs = soak_rss(child, args)
        finally:
            stopped = terminate(child, args.shutdown_grace_secs)
        if not stopped:
            raise MeasurementError("daemon ignored SIGTERM and was SIGKILLed")
        summary = {
            "harness": "rss",
            "n_tasks": args.tasks,
            "binary": str(binary),
            "fixture": str(fixture),
            "ready_secs": round(ready_secs, 3),
            "soak_secs": round(soak_secs, 3),
            "samples": count,
            "steady": True,
            "vmrss_steady_kb": steady_kb,
            "vmrss_peak_kb": peak_kb,
            "budget_reference_mb": BUDGET_REFERENCE_MB,
            "within_reference": steady_kb <= BUDGET_REFERENCE_MB * 1024,
            "scratch_dir": str(scratch),
        }
        announce(
            f"rss: steady {steady_kb} kB ({steady_kb / 1024:.1f} MB), "
            f"peak {peak_kb} kB — reference ≤ {BUDGET_REFERENCE_MB} MB "
            f"(within: {summary['within_reference']}; reference only, never a gate)"
        )
        return summary


# ------------------------------------------------------------------
# S3 — cold start
# ------------------------------------------------------------------


def display_kind(env: dict) -> str:
    if env.get("WAYLAND_DISPLAY"):
        return "wayland"
    if env.get("DISPLAY"):
        return "x11"
    raise PreconditionError(
        "no graphical session: WAYLAND_DISPLAY/DISPLAY unset or empty "
        "(slint panics at backend init headless; xvfb variants are out of scope)"
    )


def pump_first_frame(child: subprocess.Popen, deadline_secs: int, t0: float) -> float | None:
    """Reads the child's JSON log stream on a pump thread (a blocking
    read must never outlive the deadline) and returns the receipt-side
    monotonic ms since `t0` (taken immediately before the spawn) to the
    event whose *fields* carry `moment == "first-frame"` — selection on
    the structured field, never on message text."""
    import queue
    import threading

    lines: queue.Queue[str | None] = queue.Queue()

    def pump() -> None:
        assert child.stderr is not None
        for raw in child.stderr:
            lines.put(raw.decode(errors="replace"))
        lines.put(None)

    threading.Thread(target=pump, daemon=True).start()
    deadline = t0 + deadline_secs
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return None
        try:
            line = lines.get(timeout=remaining)
        except queue.Empty:
            return None
        if line is None:
            return None
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        fields = event.get("fields") or {}
        if (
            event.get("target") == "taskboard_perf"
            and fields.get("moment") == "first-frame"
        ):
            return (time.monotonic() - t0) * 1000.0


def run_cold_start(args) -> dict:
    display = display_kind(os.environ)
    binary = resolve_binary(args.taskboard, args.no_build)
    with Scratch("taskboard-cold-start-", args.keep_dir) as scratch:
        fixture = build_fixture(args.tasks, scratch, args)
        config = write_config(scratch, fixture, desktop_mode=True)

        # stderr is piped: the child's JSON log stream is the measured
        # surface. stdout goes to the scratch log (payloads, not logs).
        t0 = time.monotonic()  # immediately before the spawn (plan D4)
        with (scratch / "child-stdout.log").open("wb") as stdout:
            child = subprocess.Popen(
                [str(binary), "gui", "--config", str(config)],
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=subprocess.PIPE,
                env=child_env(),
                cwd=scratch,
            )
        try:
            first_frame_ms = pump_first_frame(child, args.deadline_secs, t0)
            if first_frame_ms is None:
                child.kill()
                child.wait()
                raise MeasurementError(
                    f'no moment="first-frame" event within {args.deadline_secs}s '
                    "of spawn:\n" + log_tail(scratch)
                )
            stopped = terminate(child, args.shutdown_grace_secs)
            if not stopped:
                raise MeasurementError(
                    "desktop process ignored SIGTERM and was SIGKILLed"
                )
        except BaseException:
            if child.poll() is None:
                child.kill()
                child.wait()
            raise
        summary = {
            "harness": "cold-start",
            "n_tasks": args.tasks,
            "binary": str(binary),
            "display": display,
            "first_frame_ms": round(first_frame_ms, 1),
            "budget_reference_s": BUDGET_REFERENCE_S,
            "within_reference": first_frame_ms <= BUDGET_REFERENCE_S * 1000,
            "scratch_dir": str(scratch),
        }
        announce(
            f"cold-start: {first_frame_ms:.1f} ms to first frame on {display} — "
            f"reference ≤ {BUDGET_REFERENCE_S} s "
            f"(within: {summary['within_reference']}; reference only, never a gate; "
            f"the moment is the mount seam's first refresh, not the presented frame)"
        )
        return summary


# ------------------------------------------------------------------
# Entry
# ------------------------------------------------------------------


def add_common(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--tasks", type=int, default=10_000,
                        help="fixture size in tasks (default 10000)")
    parser.add_argument("--fixture", default=None,
                        help="reuse an existing database file instead of building one")
    parser.add_argument("--taskboard", type=Path,
                        default=ROOT / "target/release/taskboard",
                        help="taskboard binary (default target/release/taskboard)")
    parser.add_argument("--no-build", action="store_true",
                        help="never run cargo; missing binaries are precondition errors")
    parser.add_argument("--shutdown-grace-secs", type=int, default=20,
                        help="graceful shutdown wait before SIGKILL (default 20)")
    parser.add_argument("--keep-dir", action="store_true",
                        help="keep the scratch dir on success (always kept on failure)")


def main() -> int:
    if sys.platform != "linux":
        announce(f"process smoke harnesses are Linux-only (read /proc); this is {sys.platform}")
        return EXIT_PRECONDITION

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter,
                                     epilog=__doc__)
    sub = parser.add_subparsers(dest="harness", required=True)

    rss = sub.add_parser("rss", help="S1: daemon RSS steady state (budget row 1 reference)")
    add_common(rss)
    rss.add_argument("--ready-deadline-secs", type=int, default=30,
                     help="readiness poll deadline in seconds (default 30)")
    rss.add_argument("--soak-deadline-secs", type=int, default=120,
                     help="RSS soak deadline in seconds (default 120)")
    rss.add_argument("--soak-min-secs", type=int, default=2,
                     help="minimum soak before a plateau may be declared (default 2)")
    rss.add_argument("--plateau-samples", type=int, default=8,
                     help="sliding-window samples for the plateau predicate (default 8)")
    rss.add_argument("--plateau-band-kb", type=int, default=4096,
                     help="max kB spread inside the plateau window (default 4096)")

    cold = sub.add_parser("cold-start",
                          help="S3: spawn -> first-frame event (budget row 4 reference)")
    add_common(cold)
    cold.add_argument("--deadline-secs", type=int, default=60,
                      help="first-frame event deadline in seconds (default 60)")

    args = parser.parse_args()
    try:
        if args.harness == "rss":
            summary = run_rss(args)
        else:
            summary = run_cold_start(args)
    except PreconditionError as error:
        announce(f"precondition failed: {error}")
        return EXIT_PRECONDITION
    except MeasurementError as error:
        announce(f"measurement failed: {error}")
        return EXIT_MEASUREMENT
    except subprocess.CalledProcessError as error:
        announce(f"build subprocess failed: {error}")
        return EXIT_PRECONDITION

    emit_summary(summary)
    return 0


if __name__ == "__main__":
    sys.exit(main())
