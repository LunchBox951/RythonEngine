#!/usr/bin/env python3
"""scripts/bundle.py — pre-compile and hash the release payload.

Runs BEFORE `cargo build --release`. Produces three artifacts under
`--out-dir` (default `target/bundles/`):

    game.bundle            Zipped, pre-compiled game .pyc files
    python<X><Y>.zip       Pre-compiled stdlib .pyc files
    lib-dynload/           Copy of the binary extension tree
    hashes.env             RYTHON_*_HASH env-var assignments for cargo

The hashes are baked into the `rython` binary via `build.rs` +
`option_env!`. At runtime, `release_seal.rs` recomputes the same hashes
against the deployed distribution and refuses to boot on any mismatch.

The tree-hash algorithm is byte-identical to
`crates/rython-cli/src/release_seal.rs::tree_hash`. A shared test vector
(`tests/python/test_bundle.py::test_tree_hash_vector`) pins this contract.

Usage:
    python3 scripts/bundle.py --game game --out-dir target/bundles

Implementation notes
--------------------

* `py_compile` with `PycInvalidationMode.UNCHECKED_HASH` produces .pyc
  files whose validity is keyed off the source content hash, not mtime.
  Without this, extraction timestamp drift would invalidate every
  bytecode file on the shipped machine.
* Stdlib .pyc files live in a top-level `pythonX.Y/` subdir inside the
  stdlib zip, matching CPython's `zipimport` convention.
* Binary extensions (`.so`, `.pyd`) cannot live inside a zip — CPython's
  dynamic loader needs them on-disk, so they ship under `lib-dynload/`
  and are hashed as a directory tree.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import py_compile
import shutil
import sys
import sysconfig
import tempfile
import zipfile
from pathlib import Path

from _common import STDLIB_EXCLUDES


# ── Args ──────────────────────────────────────────────────────────────────────

def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description="Pre-compile and hash the release payload")
    p.add_argument("--game", required=True, help="Path to game directory (e.g. game/)")
    p.add_argument("--out-dir", default="target/bundles",
                   help="Where to write bundle artifacts + hashes.env")
    return p.parse_args()


# ── Hashing ──────────────────────────────────────────────────────────────────

def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tree_hash(root: Path) -> str:
    """Canonical tree hash — mirrors release_seal::tree_hash in Rust.

    For every regular file under `root`, sorted by forward-slash relative
    path (bytewise ascending), feed into one outer SHA-256:
        relpath_bytes || 0x00 || sha256(file_bytes)  (raw, not hex)
    """
    files: list[tuple[str, Path]] = []
    for path in root.rglob("*"):
        if path.is_file():
            rel = path.relative_to(root).as_posix()
            files.append((rel, path))
    files.sort(key=lambda t: t[0].encode())

    outer = hashlib.sha256()
    for rel, abs_path in files:
        inner = hashlib.sha256(abs_path.read_bytes()).digest()
        outer.update(rel.encode())
        outer.update(b"\x00")
        outer.update(inner)
    return outer.hexdigest()


# ── Python version helpers ───────────────────────────────────────────────────

def stdlib_zip_name() -> str:
    """CPython convention: `python313.zip` for Python 3.13, etc."""
    ver = sysconfig.get_config_var("VERSION") or sysconfig.get_python_version()
    # VERSION is "3.13"; strip the dot to match CPython's zip archive naming.
    return f"python{ver.replace('.', '')}.zip"


def python_xy() -> str:
    """e.g. '3.13' — matches the top-level directory inside the stdlib zip."""
    return sysconfig.get_python_version()


# ── Game bundle ──────────────────────────────────────────────────────────────

def build_game_bundle(game_dir: Path, out_path: Path) -> None:
    """Compile every game/**/*.py to .pyc (UNCHECKED_HASH) and zip under
    arcnames mirroring scripts/package.py:create_bundle.
    """
    game_parent = game_dir.parent
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        compiled: list[tuple[Path, Path]] = []  # (pyc_file, arcname)
        for py_file in sorted(game_dir.rglob("*.py")):
            if "__pycache__" in py_file.parts:
                continue
            rel = py_file.relative_to(game_parent)
            pyc_rel = rel.with_suffix(".pyc")
            pyc_abs = td_path / pyc_rel
            pyc_abs.parent.mkdir(parents=True, exist_ok=True)
            py_compile.compile(
                str(py_file),
                cfile=str(pyc_abs),
                doraise=True,
                invalidation_mode=py_compile.PycInvalidationMode.UNCHECKED_HASH,
            )
            compiled.append((pyc_abs, pyc_rel))

        out_path.parent.mkdir(parents=True, exist_ok=True)
        # Deterministic zip: sort by arcname, fixed mtime.
        compiled.sort(key=lambda t: str(t[1]))
        with zipfile.ZipFile(out_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            for pyc_abs, arcname in compiled:
                info = zipfile.ZipInfo(str(arcname).replace("\\", "/"))
                info.date_time = (1980, 1, 1, 0, 0, 0)
                info.compress_type = zipfile.ZIP_DEFLATED
                zf.writestr(info, pyc_abs.read_bytes())


# ── Stdlib zip ───────────────────────────────────────────────────────────────

def build_stdlib_zip(out_path: Path) -> None:
    """Compile stdlib .py → .pyc and pack at the zip root.

    CPython's default `getpath` logic puts `pythonXY.zip` on `sys.path` and
    looks up modules via `zipimport` at the zip root (e.g. `encodings/__init__.pyc`
    — *not* `python3.14/encodings/__init__.pyc`). Nesting under a version
    subdir makes the interpreter fail with "No module named 'encodings'" on
    startup.

    Excludes test suites, tkinter, site-packages, etc. (see _common).
    """
    stdlib_src = Path(sysconfig.get_paths()["stdlib"])

    out_path.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        compiled: list[tuple[Path, str]] = []

        for py_file in sorted(stdlib_src.rglob("*.py")):
            if any(part in STDLIB_EXCLUDES for part in py_file.relative_to(stdlib_src).parts):
                continue
            rel = py_file.relative_to(stdlib_src)
            arcname = rel.with_suffix(".pyc").as_posix()
            pyc_abs = td_path / arcname
            pyc_abs.parent.mkdir(parents=True, exist_ok=True)
            try:
                py_compile.compile(
                    str(py_file),
                    cfile=str(pyc_abs),
                    doraise=True,
                    invalidation_mode=py_compile.PycInvalidationMode.UNCHECKED_HASH,
                )
            except py_compile.PyCompileError as e:
                # Some stdlib files are intentionally syntax-error fixtures
                # (e.g. lib2to3/tests/data/bom.py on older Pythons). Skip them.
                print(f"  WARNING: skipping unparseable stdlib file {py_file}: {e}",
                      file=sys.stderr)
                continue
            compiled.append((pyc_abs, arcname))

        compiled.sort(key=lambda t: t[1])
        with zipfile.ZipFile(out_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
            for pyc_abs, arcname in compiled:
                info = zipfile.ZipInfo(arcname)
                info.date_time = (1980, 1, 1, 0, 0, 0)
                info.compress_type = zipfile.ZIP_DEFLATED
                zf.writestr(info, pyc_abs.read_bytes())


# ── lib-dynload / DLLs ───────────────────────────────────────────────────────

def copy_lib_dynload(out_dir: Path) -> Path:
    """Copy the platform's extension-module directory into out_dir.

    Returns the directory that was populated (always `<out_dir>/lib-dynload`
    regardless of platform — the Rust side picks the right subdir at runtime
    based on `cfg!(windows)`).
    """
    dest = out_dir / "lib-dynload"
    if dest.exists():
        shutil.rmtree(dest)
    dest.mkdir(parents=True)

    if sys.platform.startswith("win"):
        src = Path(sys.prefix) / "DLLs"
    else:
        stdlib_src = Path(sysconfig.get_paths()["stdlib"])
        src = stdlib_src / "lib-dynload"

    if not src.is_dir():
        raise RuntimeError(f"lib-dynload source not found: {src}")

    for item in src.iterdir():
        target = dest / item.name
        if item.is_dir():
            shutil.copytree(item, target)
        else:
            shutil.copy2(item, target)
    return dest


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).parent.parent.resolve()
    game_dir = (repo_root / args.game).resolve()
    out_dir = (repo_root / args.out_dir).resolve()
    out_dir.mkdir(parents=True, exist_ok=True)

    if not game_dir.is_dir():
        sys.exit(f"ERROR: game directory not found: {game_dir}")
    project_json = game_dir / "project.json"
    if not project_json.exists():
        sys.exit(f"ERROR: project.json not found at {project_json}")

    with open(project_json) as f:
        project = json.load(f)
    entry_point = project.get("entry_point")
    if not entry_point:
        sys.exit("ERROR: project.json is missing 'entry_point' — required for sealed release")

    zip_name = stdlib_zip_name()

    print(f"Bundling game + stdlib for sealed release")
    print(f"  game:    {game_dir}")
    print(f"  out:     {out_dir}")
    print(f"  python:  {python_xy()}")

    print("  [1/3] Compiling game .py → .pyc and zipping...")
    bundle_path = out_dir / "game.bundle"
    build_game_bundle(game_dir, bundle_path)

    print(f"  [2/3] Compiling stdlib → {zip_name}...")
    stdlib_path = out_dir / zip_name
    build_stdlib_zip(stdlib_path)

    print("  [3/3] Copying lib-dynload extensions...")
    dynload_dir = copy_lib_dynload(out_dir)

    bundle_hash = sha256_file(bundle_path)
    stdlib_hash = sha256_file(stdlib_path)
    libdyn_hash = tree_hash(dynload_dir)

    env_lines = [
        f"RYTHON_BUNDLE_HASH={bundle_hash}",
        f"RYTHON_STDLIB_HASH={stdlib_hash}",
        f"RYTHON_LIBDYNLOAD_HASH={libdyn_hash}",
        f"RYTHON_STDLIB_ZIP_NAME={zip_name}",
        f"RYTHON_ENTRY_POINT={entry_point}",
        "RYTHON_SEALED=1",
        "",
    ]
    hashes_env = out_dir / "hashes.env"
    hashes_env.write_text("\n".join(env_lines))

    print()
    print(f"  bundle    sha256={bundle_hash[:12]}…  ({bundle_path.stat().st_size // 1024} KB)")
    print(f"  stdlib    sha256={stdlib_hash[:12]}…  ({stdlib_path.stat().st_size // 1024} KB)")
    print(f"  libdyn    sha256={libdyn_hash[:12]}…")
    print(f"  → {hashes_env}")


if __name__ == "__main__":
    main()
