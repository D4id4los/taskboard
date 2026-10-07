#!/usr/bin/env python3
"""Package a release artifact set for taskboard.

Shared by the release workflow (CI) and local runs, so a local run is
byte-identical to a CI leg: read the version from [workspace.package],
build taskboard-app (host target, or --target <triple>), stage the binary
plus license/README/changelog files into target/dist/, archive them
(.tar.gz default, --format zip) and write a <archive>.sha256 checksum.

Usage:
  package_release.py [--target <triple>] [--format tar.gz|zip]
  package_release.py --check [--target <triple>] [--format tar.gz|zip]

--check rebuilds nothing: it verifies the archive this invocation would
have produced — checksum matches, and it contains exactly the expected
files. Used by CI acceptance and locally after a real run.
"""

import argparse
import hashlib
import re
import shutil
import subprocess
import sys
import tarfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIST = ROOT / "target" / "dist"

STAGED_FILES = ["LICENSE-MIT", "LICENSE-APACHE", "README.org", "CHANGELOG.md"]
BINARY_NAME = "taskboard"


def read_workspace_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r"^\[workspace\.package\]", text, re.M)
    if match is None:
        sys.exit("error: no [workspace.package] table in the root Cargo.toml")
    section = text[match.end():]
    version = re.search(r'^version = "(.+?)"', section, re.M)
    if version is None:
        sys.exit("error: no version in [workspace.package]")
    return version.group(1)


def host_target():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True)
    match = re.search(r"^host: (\S+)$", out.stdout, re.M)
    if match is None:
        sys.exit("error: could not determine host triple from rustc -vV")
    return match.group(1)


def build(target):
    cmd = ["cargo", "build", "--locked", "--release", "-p", "taskboard-app"]
    if target:
        cmd += ["--target", target]
    subprocess.run(cmd, check=True, cwd=ROOT)


def built_binary(target):
    profile_dir = ROOT / "target" / (target or "") / "release"
    is_windows = "windows" in (target or host_target())
    binary = profile_dir / (BINARY_NAME + (".exe" if is_windows else ""))
    if not binary.is_file():
        sys.exit(f"error: built binary not found at {binary}")
    return binary


def archive_path(version, target, fmt):
    return DIST / f"taskboard-{version}-{target}.{fmt}"


def stage_and_archive(version, target, fmt):
    build_dir = DIST / f"taskboard-{version}-{target}"
    if build_dir.exists():
        shutil.rmtree(build_dir)
    build_dir.mkdir(parents=True)

    binary = built_binary(target)
    shutil.copy2(binary, build_dir / binary.name)
    for name in STAGED_FILES:
        shutil.copy2(ROOT / name, build_dir / name)

    archive = archive_path(version, target, fmt)
    if archive.exists():
        archive.unlink()
    if fmt == "zip":
        subprocess.run(
            ["python3", "-c", _ZIP_ONE_LINER, str(build_dir), str(archive.name)],
            cwd=DIST,
            check=True,
        )
    else:
        subprocess.run(
            ["tar", "-czf", archive.name, "-C", str(build_dir.parent), build_dir.name],
            cwd=DIST,
            check=True,
        )
    return archive, build_dir


_ZIP_ONE_LINER = (
    "import shutil, sys; shutil.make_archive(sys.argv[2][:-4], 'zip', sys.argv[1])"
)


def sha256_of(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_members(version, target):
    is_windows = "windows" in target
    binary = BINARY_NAME + (".exe" if is_windows else "")
    prefix = f"taskboard-{version}-{target}"
    return [f"{prefix}/{binary}"] + [f"{prefix}/{name}" for name in STAGED_FILES]


def check_archive(version, target, fmt):
    archive = archive_path(version, target, fmt)
    if not archive.is_file():
        sys.exit(f"error: archive not found: {archive}")
    checksum_file = Path(str(archive) + ".sha256")
    if not checksum_file.is_file():
        sys.exit(f"error: checksum file not found: {checksum_file}")

    expected_line = f"{sha256_of(archive)}  {archive.name}\n"
    actual = checksum_file.read_text(encoding="utf-8")
    if actual != expected_line:
        sys.exit(f"error: checksum mismatch for {archive.name}")

    prefix = f"taskboard-{version}-{target}"
    if fmt == "zip":
        import zipfile

        with zipfile.ZipFile(archive) as zf:
            members = sorted(zf.namelist())
    else:
        with tarfile.open(archive) as tf:
            members = sorted(
                m.name for m in tf.getmembers() if m.isfile()
            )
    if members != sorted(expected_members(version, target)):
        sys.exit(
            f"error: unexpected archive contents:\n  {members}\nexpected:\n"
            f"  {sorted(expected_members(version, target))}"
        )
    print(f"check passed: {archive.name}")


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--target", help="target triple (default: host)")
    parser.add_argument("--format", choices=["tar.gz", "zip"], default="tar.gz")
    parser.add_argument("--check", action="store_true", help="verify an existing archive")
    args = parser.parse_args(argv[1:])

    version = read_workspace_version()
    target = args.target or host_target()

    if args.check:
        check_archive(version, target, args.format)
        return 0

    if shutil.which("cargo") is None:
        sys.exit("error: cargo not found on PATH")

    archive, build_dir = stage_and_archive(version, target, args.format)
    checksum_file = Path(str(archive) + ".sha256")
    checksum_file.write_text(f"{sha256_of(archive)}  {archive.name}\n", encoding="utf-8")
    print(f"packaged: {archive}")
    print(f"checksum: {checksum_file}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
