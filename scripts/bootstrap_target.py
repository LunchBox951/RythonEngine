#!/usr/bin/env python3
"""scripts/bootstrap_target.py — Download and extract a pinned python-build-standalone
distribution for a target platform/arch.

Usage:
    python3 scripts/bootstrap_target.py <platform> <arch>

Example:
    python3 scripts/bootstrap_target.py windows x86_64
    python3 scripts/bootstrap_target.py linux x86_64

Pins live in scripts/python_standalone_pins.json. Tarballs are downloaded to
vendor/python/<platform>-<arch>/.download/ and extracted to
vendor/python/<platform>-<arch>/ (so the install tree is at
vendor/python/<platform>-<arch>/python/).

Idempotent: if the target directory already exists and contains the expected
marker file (.bootstrap-ok with the pinned sha256), nothing is re-downloaded.
"""

import argparse
import hashlib
import json
import shutil
import sys
import tarfile
import urllib.request
from pathlib import Path


VALID_PLATFORMS = ("linux", "windows", "macos")
VALID_ARCHES = ("x86_64", "aarch64")

# tarfile.extractall(filter=...) was added in 3.12 and is the only safe
# extraction path we use. Fail fast with a clear message rather than letting
# `extract()` raise an opaque TypeError on older hosts.
MIN_PYTHON = (3, 12)


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("platform", choices=VALID_PLATFORMS)
    p.add_argument("arch", choices=VALID_ARCHES)
    p.add_argument("--pins", default=None,
                   help="Path to pins JSON (default: scripts/python_standalone_pins.json)")
    p.add_argument("--vendor-root", default=None,
                   help="Vendor root (default: vendor/python)")
    return p.parse_args()


def load_pins(pins_path: Path) -> dict:
    with open(pins_path) as f:
        return json.load(f)


def sha256_of(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def download(url: str, dest: Path) -> None:
    """Stream `url` to `dest` atomically.

    Writes to a sibling `<dest>.partial` and renames into place on success, so
    a Ctrl-C / network interruption never leaves `dest` looking complete-but-
    corrupt. Any pre-existing partial is removed first.
    """
    dest.parent.mkdir(parents=True, exist_ok=True)
    partial = dest.with_suffix(dest.suffix + ".partial")
    if partial.exists():
        partial.unlink()
    print(f"  downloading {url}")
    try:
        with urllib.request.urlopen(url) as resp, open(partial, "wb") as out:
            total = int(resp.headers.get("Content-Length") or 0)
            read = 0
            last_pct = -1
            while True:
                chunk = resp.read(1 << 20)
                if not chunk:
                    break
                out.write(chunk)
                read += len(chunk)
                if total:
                    pct = read * 100 // total
                    if pct != last_pct and pct % 10 == 0:
                        print(f"    {pct}%  ({read // (1 << 20)}/{total // (1 << 20)} MB)")
                        last_pct = pct
        partial.replace(dest)
    except BaseException:
        # Includes KeyboardInterrupt — clean up the partial so the next run
        # does not see a half-written file at `dest`.
        if partial.exists():
            partial.unlink()
        raise


def extract(archive: Path, into: Path) -> None:
    print(f"  extracting into {into}")
    into.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive, "r:gz") as tf:
        # filter="data" blocks absolute paths, .. traversal, and unsafe symlinks.
        # Available since Python 3.12; becomes the default in 3.14.
        tf.extractall(into, filter="data")


def write_marker(target_dir: Path, sha: str, meta: dict) -> None:
    marker = target_dir / ".bootstrap-ok"
    payload = {"sha256": sha, **meta}
    marker.write_text(json.dumps(payload, indent=2) + "\n")


def read_marker(target_dir: Path) -> dict | None:
    marker = target_dir / ".bootstrap-ok"
    if not marker.is_file():
        return None
    try:
        return json.loads(marker.read_text())
    except (OSError, json.JSONDecodeError):
        return None


def main() -> int:
    if sys.version_info < MIN_PYTHON:
        sys.exit(
            f"ERROR: bootstrap_target.py requires Python >= {MIN_PYTHON[0]}.{MIN_PYTHON[1]} "
            f"(found {sys.version_info.major}.{sys.version_info.minor}). "
            "tarfile's `filter='data'` safe-extract argument was added in 3.12."
        )
    args = parse_args()
    repo_root = Path(__file__).parent.parent.resolve()
    pins_path = Path(args.pins) if args.pins else repo_root / "scripts" / "python_standalone_pins.json"
    vendor_root = Path(args.vendor_root) if args.vendor_root else repo_root / "vendor" / "python"

    pins = load_pins(pins_path)
    key = f"{args.platform}-{args.arch}"
    target = pins["targets"].get(key)
    if not target:
        sys.exit(f"ERROR: no pin for {key} in {pins_path}")

    url = target["url"]
    expected_sha = target["sha256"]

    target_dir = vendor_root / key
    install_marker = target_dir / "python"  # expected top-level after extract

    existing = read_marker(target_dir)
    if existing and existing.get("sha256") == expected_sha and install_marker.is_dir():
        print(f"[bootstrap] {key} already populated ({install_marker}). Skipping.")
        return 0

    print(f"[bootstrap] {key} -> {target_dir}")
    if target_dir.exists():
        print(f"  removing stale tree: {target_dir}")
        shutil.rmtree(target_dir)

    download_dir = target_dir / ".download"
    tarball = download_dir / url.rsplit("/", 1)[-1]
    download(url, tarball)

    actual_sha = sha256_of(tarball)
    if actual_sha != expected_sha:
        sys.exit(
            f"ERROR: sha256 mismatch for {tarball.name}\n"
            f"  expected: {expected_sha}\n"
            f"  actual:   {actual_sha}"
        )
    print(f"  sha256 ok: {actual_sha}")

    extract(tarball, target_dir)

    # install_only tarballs extract to <target>/python/...
    if not install_marker.is_dir():
        sys.exit(f"ERROR: extracted tree has no python/ at {install_marker}")

    # Free up space: the .download cache is no longer needed.
    shutil.rmtree(download_dir, ignore_errors=True)

    write_marker(target_dir, expected_sha, {
        "platform": args.platform,
        "arch": args.arch,
        "cpython_version": pins.get("cpython_version"),
        "python_version": pins.get("python_version"),
        "release_tag": pins.get("release_tag"),
        "rust_triple": target.get("rust_triple"),
        "source_url": url,
    })

    print(f"[bootstrap] {key} ready at {install_marker}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
