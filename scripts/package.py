#!/usr/bin/env python3
"""scripts/package.py — Assemble a self-contained RythonEngine release distribution.

Usage:
    python3 scripts/package.py \\
        --platform windows --arch x86_64 \\
        --rust-triple x86_64-pc-windows-msvc \\
        --target-python vendor/python/windows-x86_64/python \\
        --bundle-dir  target/bundles/windows-x86_64 \\
        --game game --out dist/windows-x86_64

The script expects:
  * The release binary at target/<rust-triple>/release/rython[.exe] — built by
    `cargo zigbuild --release --target <rust-triple>` (see the Makefile).
  * A vendored python-build-standalone tree at --target-python — populated by
    `scripts/bootstrap_target.py <platform> <arch>`.
  * A pre-built, pre-hashed bundle tree at --bundle-dir — populated by
    `scripts/bundle.py --target-python <same-tree>`. The sealed binary has
    hashes for these artifacts baked in at compile time; this script
    installs them at the exact paths `release_seal::verify` checks:

        POSIX:   python/lib/<pythonXY.zip>, python/lib/<libpython soname>,
                 python/lib/pythonX.Y/lib-dynload/
        Windows: python/lib/<pythonXY.zip>, <libpython soname> at dist root,
                 python/DLLs/

Zero host tools are consulted. Everything about the bundled Python runtime
is read from the vendored target tree, so output is deterministic regardless
of the developer's host.

macOS is host-only: targeting macOS from a non-macOS host exits with an
error because `install_name_tool` is needed and cannot be faked cross-host.
"""

from __future__ import annotations

import argparse
import json
import platform as host_platform
import re
import shutil
import subprocess
import sys
from pathlib import Path


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Package a RythonEngine game for distribution",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--platform", required=True, choices=["linux", "windows", "macos"],
                   help="Target platform")
    p.add_argument("--arch", required=True, choices=["x86_64", "aarch64"],
                   help="Target CPU architecture")
    p.add_argument("--game", required=True,
                   help="Path to game directory (e.g. game/)")
    p.add_argument("--out", required=True,
                   help="Output base directory (e.g. dist/linux-x86_64)")
    p.add_argument("--target-python", required=True,
                   help="Path to vendored python install tree "
                        "(e.g. vendor/python/windows-x86_64/python)")
    p.add_argument("--bundle-dir", required=True,
                   help="Path to sealed bundle artifacts "
                        "(e.g. target/bundles/windows-x86_64) — produced by "
                        "scripts/bundle.py against the same --target-python tree. "
                        "Must agree with the hashes baked into the binary.")
    p.add_argument("--rust-triple", required=True,
                   help="Rust target triple used for cargo (e.g. x86_64-pc-windows-msvc). "
                        "Determines target/<triple>/release/ binary location.")
    return p.parse_args()


# ── Target Python discovery ──────────────────────────────────────────────────

class TargetPython:
    """Describes the vendored python-build-standalone tree for one target.

    Intentionally mirrors `scripts/bundle.py::TargetPython` — both scripts
    must agree on what the stdlib/lib-dynload/libpython look like, or the
    sealed hashes would not match what lands in the dist.
    """

    def __init__(self, root: Path, target_platform: str):
        self.root = root                                  # .../vendor/python/<key>/python
        self.target_platform = target_platform            # linux|windows|macos
        self.version_short = self._detect_version()       # e.g. "3.12"

    def _detect_version(self) -> str:
        if self.target_platform == "windows":
            for m in sorted(self.root.glob("python3*.dll")):
                # Skip the "abi3 shim" python3.dll which has no minor version.
                mo = re.fullmatch(r"python3(\d+)\.dll", m.name)
                if mo:
                    return f"3.{mo.group(1)}"
            raise RuntimeError(f"Could not detect Python version in {self.root} (no python3XX.dll)")
        for m in sorted(self.root.glob("lib/python3.*")):
            mo = re.fullmatch(r"python(3\.\d+)", m.name)
            if mo:
                return mo.group(1)
        raise RuntimeError(f"Could not detect Python version in {self.root} (no lib/python3.X/)")

    def zip_name(self) -> str:
        return f"python{self.version_short.replace('.', '')}.zip"

    @property
    def libpython_files(self) -> list[Path]:
        """Files to copy next to the executable / into the runtime tree."""
        if self.target_platform == "windows":
            # Versioned libpython DLL is required. python3.dll (abi3 shim) and
            # vcruntime140*.dll (MSVC runtime needed by extension modules) are
            # shipped alongside it — bundling them means the target machine
            # does not need a separate VC++ Redistributable install.
            versioned = self.root / f"python{self.version_short.replace('.', '')}.dll"
            if not versioned.is_file():
                raise RuntimeError(f"missing versioned libpython at {versioned}")
            optional = ["python3.dll", "vcruntime140.dll", "vcruntime140_1.dll"]
            return [versioned] + [self.root / n for n in optional if (self.root / n).is_file()]
        if self.target_platform == "macos":
            candidates = list((self.root / "lib").glob("libpython*.dylib"))
            if not candidates:
                raise RuntimeError(f"No libpython*.dylib in {self.root / 'lib'}")
            return candidates
        # linux
        candidates = list((self.root / "lib").glob("libpython*.so*"))
        if not candidates:
            raise RuntimeError(f"No libpython*.so* in {self.root / 'lib'}")
        return candidates

    def versioned_libpython(self) -> Path:
        """The single real (non-symlink) libpython file that `bundle.py::hash_libpython`
        hashed. Must end up at `python/lib/<this filename>` (POSIX) or at the
        dist root with this filename (Windows) so `release_seal::libpython_path`
        resolves it."""
        if self.target_platform == "windows":
            return self.root / f"python{self.version_short.replace('.', '')}.dll"
        subdir = self.root / "lib"
        if self.target_platform == "macos":
            matches = sorted(p for p in subdir.glob("libpython*.dylib")
                             if p.is_file() and not p.is_symlink())
        else:
            matches = sorted(p for p in subdir.glob("libpython*.so*")
                             if p.is_file() and not p.is_symlink())
        if not matches:
            raise RuntimeError(f"No non-symlink libpython in {subdir}")
        return matches[0]


# ── Runtime layout: copy libpython + patch binary ────────────────────────────

def install_runtime(
    tp: TargetPython,
    dest_binary: Path,
    dest_dir: Path,
) -> None:
    """Copy the target Python runtime into dest_dir and patch the binary.

    This does NOT copy the stdlib or lib-dynload — those come from the sealed
    bundle via `install_sealed_artifacts`. The seal covers everything CPython
    loads at the Python level; this function handles the dynamic-linker layer
    (libpython, RPATH, @executable_path references, Windows vcruntime).
    """
    if tp.target_platform == "linux":
        _install_runtime_linux(tp, dest_binary, dest_dir)
    elif tp.target_platform == "macos":
        _install_runtime_macos(tp, dest_binary, dest_dir)
    elif tp.target_platform == "windows":
        _install_runtime_windows(tp, dest_binary, dest_dir)
    else:
        raise ValueError(f"unsupported platform: {tp.target_platform}")


def _install_runtime_linux(tp: TargetPython, dest_binary: Path, dest_dir: Path) -> None:
    dest_python_lib = dest_dir / "python" / "lib"
    dest_python_lib.mkdir(parents=True, exist_ok=True)

    # We install ONLY the single versioned libpython that the seal hashes —
    # no unversioned symlinks. The binary's DT_NEEDED points at the versioned
    # soname (set by PyO3 / python-build-standalone at link time), so an
    # unversioned `libpython3.so` sitting alongside would be unused at
    # load time but *would* be an extra file in the dist that release_seal.rs
    # does not know about. Keeping the dist minimal keeps the seal tight.
    src = tp.versioned_libpython()
    dst = dest_python_lib / src.name
    shutil.copy2(src, dst)
    # Strip to match the hash in hashes.env (bundle.py hashed post-strip bytes).
    _strip_libpython(dst)
    print(f"         libpython: {src.name} (stripped)")

    # lib64 -> lib symlink for multiarch Python dlopen compatibility.
    lib64 = dest_dir / "python" / "lib64"
    if not lib64.exists():
        lib64.symlink_to("lib")

    _patch_rpath_linux(dest_binary, "$ORIGIN/python/lib")


def _strip_libpython(libpython: Path) -> None:
    """Strip the real (non-symlink) libpython. python-build-standalone's
    Linux install_only tarballs include debug info which bloats the dist by
    ~200 MB with no runtime benefit. `bundle.py::hash_libpython` pre-hashed
    the post-strip bytes, so this strip is not just a size optimisation —
    it is load-bearing for seal verification.
    """
    try:
        subprocess.run(
            ["strip", "--strip-unneeded", str(libpython)],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except FileNotFoundError:
        sys.exit(
            "ERROR: `strip` not found on PATH but required to install a sealed "
            "Linux libpython (the sealed hash is against the stripped bytes). "
            "Install binutils: `apt install binutils` / `pacman -S binutils`."
        )
    except subprocess.CalledProcessError as e:
        sys.exit(f"ERROR: strip failed on {libpython}: {e}")


def _install_runtime_macos(tp: TargetPython, dest_binary: Path, dest_dir: Path) -> None:
    if host_platform.system() != "Darwin":
        sys.exit(
            "ERROR: packaging a macOS target requires a macOS host.\n"
            "       `install_name_tool` is not portable across hosts — run this on a Mac."
        )
    dest_python_lib = dest_dir / "python" / "lib"
    dest_python_lib.mkdir(parents=True, exist_ok=True)

    dylib_src = tp.versioned_libpython()
    soname = dylib_src.name
    dest_dylib = dest_python_lib / soname
    shutil.copy2(dylib_src, dest_dylib)
    print(f"         libpython: {soname}")

    # Retarget the dylib's own install name.
    subprocess.run(
        ["install_name_tool", "-id", f"@loader_path/{soname}", str(dest_dylib)],
        check=True,
    )
    # Rewrite the binary's reference. python-build-standalone builds use
    # @executable_path/../lib/<soname> as the install name on the binary's side,
    # but the actual reference depends on how cargo linked — discover via otool.
    try:
        out = subprocess.check_output(["otool", "-L", str(dest_binary)], text=True)
    except (FileNotFoundError, subprocess.CalledProcessError) as e:
        raise RuntimeError(f"otool -L failed on {dest_binary}: {e}") from e
    old_ref: str | None = None
    for line in out.splitlines():
        stripped = line.strip()
        if soname in stripped and not stripped.endswith(":"):
            old_ref = stripped.split()[0]
            break
    if old_ref is None:
        raise RuntimeError(
            f"Could not find {soname} in otool output for {dest_binary}. "
            "Was the binary linked against the vendored libpython?"
        )
    subprocess.run(
        ["install_name_tool", "-change", old_ref,
         f"@executable_path/python/lib/{soname}", str(dest_binary)],
        check=True,
    )


def _install_runtime_windows(tp: TargetPython, dest_binary: Path, dest_dir: Path) -> None:
    # Windows resolves DLLs from the exe's directory — drop them in beside it.
    # Versioned libpython (`python313.dll`) lands at the dist root where
    # `release_seal::libpython_path` expects it. Optional DLLs (python3 shim,
    # vcruntime) are sibling runtime deps — not sealed, but required for
    # startup on hosts without a VC++ redistributable.
    for src in tp.libpython_files:
        shutil.copy2(src, dest_dir / src.name)
    print(f"         libpython: {', '.join(f.name for f in tp.libpython_files)}")


def _patch_rpath_linux(binary: Path, rpath: str) -> None:
    try:
        subprocess.run(
            ["patchelf", "--set-rpath", rpath, str(binary)],
            check=True,
        )
        print(f"         RPATH set to: {rpath}")
    except FileNotFoundError:
        sys.exit(
            "ERROR: patchelf not found but required for Linux dist.\n"
            "Install patchelf:  pacman -S patchelf  /  apt install patchelf"
        )
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"patchelf failed: {e}") from e


# ── Sealed artifact installation ─────────────────────────────────────────────

def install_sealed_artifacts(
    bundle_dir: Path,
    dest_dir: Path,
    tp: TargetPython,
) -> None:
    """Install the pre-built sealed artifacts at the exact paths
    `release_seal::verify` checks. Any layout drift here would cause a
    post-install `SealError::Io`/`*Mismatch` that looks like tampering.
    """
    # 1. game.bundle -> <dest>/game.bundle
    game_bundle_src = bundle_dir / "game.bundle"
    if not game_bundle_src.is_file():
        sys.exit(f"ERROR: game.bundle not found in {bundle_dir} — run scripts/bundle.py first")
    shutil.copy2(game_bundle_src, dest_dir / "game.bundle")
    size_kb = game_bundle_src.stat().st_size // 1024
    print(f"         bundle: game.bundle ({size_kb} KB)")

    # 2. pythonXY.zip -> <dest>/python/lib/<name>
    #    Same path on POSIX and Windows — release_seal::stdlib_zip_path does
    #    not branch on cfg!(windows).
    zip_name = tp.zip_name()
    zip_src = bundle_dir / zip_name
    if not zip_src.is_file():
        sys.exit(f"ERROR: stdlib zip not found at {zip_src} — run scripts/bundle.py first")
    zip_dest = dest_dir / "python" / "lib" / zip_name
    zip_dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(zip_src, zip_dest)
    size_kb = zip_src.stat().st_size // 1024
    print(f"         stdlib: {zip_dest.relative_to(dest_dir)} ({size_kb} KB)")

    # 3. lib-dynload/ -> platform-specific path release_seal.rs scans.
    dyn_src = bundle_dir / "lib-dynload"
    if not dyn_src.is_dir():
        sys.exit(f"ERROR: lib-dynload not found at {dyn_src} — run scripts/bundle.py first")
    if tp.target_platform == "windows":
        dyn_dest = dest_dir / "python" / "DLLs"
    else:
        dyn_dest = dest_dir / "python" / "lib" / f"python{tp.version_short}" / "lib-dynload"
    if dyn_dest.exists():
        shutil.rmtree(dyn_dest)
    dyn_dest.parent.mkdir(parents=True, exist_ok=True)
    # symlinks=False dereferences (mirrors bundle.py; sealed tree is already
    # symlink-free by construction, but staying defensive on the install side).
    shutil.copytree(dyn_src, dyn_dest, symlinks=False)
    n = sum(1 for p in dyn_dest.rglob("*") if p.is_file())
    print(f"         lib-dynload: {dyn_dest.relative_to(dest_dir)} ({n} files)")


# ── Game data and project.json ───────────────────────────────────────────────

def copy_game_data(game_dir: Path, dest_dir: Path) -> None:
    """Mirror non-Python game data to dest_dir/<game-dirname>/.

    Scripts reference resources via paths like "game/assets/music/x.mp3" and
    "game/ui/main_menu.json", relative to the process cwd. Preserving the
    top-level game directory under the dist root keeps those paths valid when
    the game is launched from the dist directory.
    """
    data_root = dest_dir / game_dir.name
    data_root.mkdir(parents=True, exist_ok=True)

    copied = 0
    for src in game_dir.rglob("*"):
        if src.is_dir():
            continue
        if src.suffix == ".py":
            continue
        if "__pycache__" in src.parts:
            continue
        rel = src.relative_to(game_dir)
        dest = data_root / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)
        copied += 1

    size_mb = sum(f.stat().st_size for f in data_root.rglob("*") if f.is_file()) // (1024 * 1024)
    print(f"         game data: {game_dir.name}/ ({size_mb} MB, {copied} files)")


def copy_project_json(game_dir: Path, dest_dir: Path) -> dict:
    src = game_dir / "project.json"
    if not src.exists():
        raise FileNotFoundError(f"project.json not found at {src}")
    with open(src) as f:
        config = json.load(f)
    shutil.copy2(src, dest_dir / "project.json")
    return config


# ── Slugify helper ───────────────────────────────────────────────────────────

def slugify(name: str) -> str:
    """Turn a game name into a filesystem-safe binary name (strip spaces)."""
    return "".join(c for c in name if c.isalnum() or c in "-_")


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
    args = parse_args()
    repo_root = Path(__file__).parent.parent.resolve()
    game_dir = (repo_root / args.game).resolve()

    if not game_dir.is_dir():
        sys.exit(f"ERROR: game directory not found: {game_dir}")

    target_python_root = Path(args.target_python).resolve()
    if not target_python_root.is_dir():
        sys.exit(
            f"ERROR: target python tree not found at {target_python_root}\n"
            f"       Run: python3 scripts/bootstrap_target.py {args.platform} {args.arch}"
        )
    tp = TargetPython(target_python_root, args.platform)

    bundle_dir = Path(args.bundle_dir).resolve()
    if not bundle_dir.is_dir():
        sys.exit(
            f"ERROR: --bundle-dir not found: {bundle_dir}\n"
            "       Run `scripts/bundle.py` first "
            "(Makefile's `dist` target handles this)."
        )

    project_json_path = game_dir / "project.json"
    if not project_json_path.exists():
        sys.exit(f"ERROR: project.json not found at {project_json_path}")
    with open(project_json_path) as f:
        project = json.load(f)

    game_name = slugify(project.get("name", "game") or "game")
    suffix = ".exe" if args.platform == "windows" else ""
    binary_name = game_name + suffix

    dest_dir = Path(args.out) / game_name
    if dest_dir.exists():
        shutil.rmtree(dest_dir)
    dest_dir.mkdir(parents=True)

    print(f"Packaging  {game_name}  ->  {args.platform}-{args.arch}  (python {tp.version_short})")
    print(f"Output:    {dest_dir}")
    print()

    # ── 1. Locate the release binary ──────────────────────────────────────────
    print("  [1/5] Locating release binary...")
    src_binary_name = "rython.exe" if args.platform == "windows" else "rython"
    binary_src = repo_root / "target" / args.rust_triple / "release" / src_binary_name
    if not binary_src.exists():
        sys.exit(
            f"ERROR: release binary not found at {binary_src}\n"
            "       Build via `make dist PLATFORM=... ARCH=...` which runs the correct "
            "cargo invocation."
        )
    print(f"         found: {binary_src}")

    dest_binary = dest_dir / binary_name
    shutil.copy2(binary_src, dest_binary)
    if args.platform != "windows":
        dest_binary.chmod(dest_binary.stat().st_mode | 0o111)

    # ── 2. Install target Python runtime (libpython, vcruntime, RPATH) ───────
    print("  [2/5] Installing Python runtime...")
    install_runtime(tp, dest_binary, dest_dir)

    # ── 3. Install sealed bundle artifacts at release_seal.rs paths ──────────
    print("  [3/5] Installing sealed bundle artifacts...")
    install_sealed_artifacts(bundle_dir, dest_dir, tp)

    # ── 4. Copy project.json and game data ───────────────────────────────────
    print("  [4/5] Copying project.json and game data...")
    copy_project_json(game_dir, dest_dir)
    copy_game_data(game_dir, dest_dir)

    # ── 5. Summary ───────────────────────────────────────────────────────────
    print("  [5/5] Summarising...")
    total_mb = sum(f.stat().st_size for f in dest_dir.rglob("*") if f.is_file()) // (1024 * 1024)
    print()
    print(f"Done. Distribution size: {total_mb} MB")
    print(f"Launch:  {dest_dir / binary_name}")


if __name__ == "__main__":
    main()
