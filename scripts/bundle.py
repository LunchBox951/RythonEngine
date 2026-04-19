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
import os
import py_compile
import shutil
import sys
import sysconfig
import tempfile
import zipfile
from pathlib import Path

from _common import PTH_SUFFIX, STDLIB_EXCLUDES


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

    Both sides *reject* symlinks with a hard error rather than silently
    skipping them. Rust's `collect_files` returns `SealError::UnexpectedPath`
    on any `is_symlink()` entry; this function raises `SystemExit`. The
    symmetry closes a post-install injection gap: without it, an attacker
    dropping `evil.so -> /tmp/payload.so` into a hashed tree would leave the
    hash matching (silent skip) while CPython still loaded the shadowed
    module. `bundle.py::assert_no_symlinks` guarantees the built tree is
    already symlink-free, so this raise is a defense-in-depth net.

    For every regular file found, sorted by forward-slash relative path
    (bytewise ascending), feed into one outer SHA-256:
        relpath_bytes || 0x00 || sha256(file_bytes)  (raw, not hex)
    """
    files: list[tuple[str, Path]] = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        base = Path(dirpath)
        # Reject symlinked subdirectories (reported by os.walk under
        # dirnames with followlinks=False, not descended but still listed).
        for dname in dirnames:
            if (base / dname).is_symlink():
                sys.exit(
                    f"FATAL: symlink in sealed tree at {base / dname} — refusing to hash.\n"
                    "Rust's collect_files rejects these with UnexpectedPath; matching here."
                )
        for fname in filenames:
            fpath = base / fname
            if fpath.is_symlink():
                sys.exit(
                    f"FATAL: symlink in sealed tree at {fpath} — refusing to hash.\n"
                    "Rust's collect_files rejects these with UnexpectedPath; matching here."
                )
            rel = fpath.relative_to(root).as_posix()
            # Enforce byte-identical UTF-8 with Rust's `to_str()` check in
            # `collect_files`. On POSIX with a non-UTF-8 locale, Python decodes
            # filename bytes using surrogateescape — the resulting `str`
            # contains surrogate code points that `.encode("utf-8")` refuses.
            # Without this explicit check, the failure surfaces as a buried
            # `UnicodeEncodeError` in the sort key lambda; with it, the
            # developer sees exactly which file broke the UTF-8 contract.
            try:
                rel.encode("utf-8")
            except UnicodeEncodeError:
                sys.exit(
                    f"FATAL: non-UTF-8 filename in sealed tree at {fpath} — "
                    "refusing to hash.\n"
                    "Rust's collect_files rejects non-UTF-8 names with "
                    "InvalidData; the cross-language hash contract requires "
                    "byte-identical UTF-8 relpaths."
                )
            files.append((rel, fpath))
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

    Aborts if any `.pth` file is copied. The extension-module directory is on
    `sys.path` at runtime, so a stray `.pth` file shipped here would be
    processed by `site.py` and could inject attacker-controlled directories.

    Symlink handling: we *dereference* every symlink at copy time so the
    bundle contains only real files. This is load-bearing for hash agreement.
    `release_seal::tree_hash` on the Rust side uses `DirEntry::file_type()`,
    which on Linux returns the lstat-style type — symlinked directories fall
    into neither `is_dir()` nor `is_file()` and are silently skipped. If any
    symlink survived into the shipped `lib-dynload/`, Rust would skip it
    while `bundle.py::tree_hash` (also `followlinks=False`) would also skip
    it — *but the dynamic loader would then fail* to open the missing
    extension. Dereferencing here guarantees real files at both hash time
    and load time.
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
            # symlinks=False (explicit) dereferences any symlinks encountered
            # during recursion, writing real files at the destination.
            # dirs_exist_ok=False because we just rmtree'd + mkdir'd dest.
            shutil.copytree(item, target, symlinks=False)
        else:
            # follow_symlinks=True (default, made explicit) dereferences
            # top-level file symlinks so the bundled file is a real file.
            shutil.copy2(item, target, follow_symlinks=True)

    assert_no_symlinks(dest, "lib-dynload")
    assert_no_pth_files(dest, "lib-dynload")
    return dest


def assert_no_symlinks(tree: Path, label: str) -> None:
    """Fail loudly if any symlink survives under `tree`.

    `release_seal::tree_hash` on the Rust side silently skips symlinks via
    `DirEntry::file_type()`; a symlink in the bundle would be invisible to
    the hash but visible to the dynamic loader — a gap an attacker could
    exploit to swap in an untracked shared object. Dereferencing at copy
    time + this assertion close that gap.
    """
    offenders = [p for p in tree.rglob("*") if p.is_symlink()]
    if offenders:
        listing = "\n  ".join(str(p) for p in offenders)
        sys.exit(
            f"FATAL: symlink(s) survived into {label} tree — refusing to seal:\n"
            f"  {listing}\n"
            "Symlinks are silently skipped by release_seal::tree_hash on the\n"
            "Rust side, creating an untracked file-set gap in the seal."
        )


def assert_no_pth_files(tree: Path, label: str) -> None:
    """Fail loudly if any `.pth` file exists under `tree`.

    Called on every directory that will land on `sys.path` in the sealed
    release layout. A `.pth` file in any of these directories is processed
    by `site.py` at interpreter startup and can inject arbitrary
    attacker-controlled entries into `sys.path`, defeating the whole seal.
    """
    offenders = [p for p in tree.rglob(f"*{PTH_SUFFIX}") if p.is_file()]
    if offenders:
        listing = "\n  ".join(str(p) for p in offenders)
        sys.exit(
            f"FATAL: .pth file(s) found in {label} tree — refusing to seal:\n"
            f"  {listing}\n"
            "A .pth file at this location is processed by site.py at startup\n"
            "and can inject attacker-controlled directories into sys.path."
        )


# ── libpython shared object ──────────────────────────────────────────────────

def hash_libpython() -> tuple[str, str]:
    """Hash the libpython shared object that will be bundled into the release
    distribution, returning `(hex_digest, soname)`.

    The soname must match exactly what `scripts/package.py` copies into
    `python/lib/` (POSIX) or the dist root (Windows). Both scripts use the
    same `sysconfig` probe so their views stay aligned.

    The dynamic linker resolves this file before `main()` runs, so a
    tampered libpython with `__attribute__((constructor))` executes code
    before any Rust-side seal check can run. Verifying it in-process is
    defence-in-depth: we cannot prevent the pre-`main()` execution, but we
    can guarantee that a tampered distribution refuses to continue past the
    seal check.
    """
    if sys.platform.startswith("win"):
        ver = sysconfig.get_config_var("VERSION") or sysconfig.get_python_version()
        digits = ver.replace(".", "")
        soname = f"python{digits}.dll"
        candidate = Path(sys.prefix) / soname
        if not candidate.exists():
            sys.exit(f"FATAL: could not locate libpython for sealing at {candidate}")
        return sha256_file(candidate), soname

    libdir = sysconfig.get_config_var("LIBDIR") or ""
    instsoname = sysconfig.get_config_var("INSTSONAME") or ""
    if libdir and instsoname:
        candidate = Path(libdir) / instsoname
        if candidate.exists():
            # Match package.py:_patch_rpath_linux, which resolves symlinks
            # before copying: we hash the real versioned file and pin its
            # soname as the expected filename at the dist path.
            return sha256_file(candidate.resolve()), instsoname

    # Fallback: scan sys.prefix/lib for libpython*.so*
    for p in sorted((Path(sys.prefix) / "lib").glob("libpython*.so*")):
        if p.is_file() and not p.is_symlink():
            return sha256_file(p), p.name

    sys.exit(
        "FATAL: could not locate libpython shared object for sealing.\n"
        "Ensure the Python development libraries are installed."
    )


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

    print("  [1/4] Compiling game .py → .pyc and zipping...")
    bundle_path = out_dir / "game.bundle"
    build_game_bundle(game_dir, bundle_path)

    print(f"  [2/4] Compiling stdlib → {zip_name}...")
    stdlib_path = out_dir / zip_name
    build_stdlib_zip(stdlib_path)

    print("  [3/4] Copying lib-dynload extensions...")
    dynload_dir = copy_lib_dynload(out_dir)

    print("  [4/4] Hashing libpython runtime...")
    libpython_hash, libpython_soname = hash_libpython()

    bundle_hash = sha256_file(bundle_path)
    stdlib_hash = sha256_file(stdlib_path)
    libdyn_hash = tree_hash(dynload_dir)

    env_lines = [
        f"RYTHON_BUNDLE_HASH={bundle_hash}",
        f"RYTHON_STDLIB_HASH={stdlib_hash}",
        f"RYTHON_LIBDYNLOAD_HASH={libdyn_hash}",
        f"RYTHON_LIBPYTHON_HASH={libpython_hash}",
        f"RYTHON_LIBPYTHON_SONAME={libpython_soname}",
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
    print(f"  libpython sha256={libpython_hash[:12]}…  ({libpython_soname})")
    print(f"  → {hashes_env}")


if __name__ == "__main__":
    main()
