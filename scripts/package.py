#!/usr/bin/env python3
"""scripts/package.py — Assemble a self-contained RythonEngine release distribution.

Usage:
    python3 scripts/package.py \\
        --platform windows --arch x86_64 \\
        --rust-triple x86_64-pc-windows-msvc \\
        --target-python vendor/python/windows-x86_64/python \\
        --game game --out dist/windows-x86_64

The script expects:
  * The release binary at target/<rust-triple>/release/rython[.exe] — built by
    `cargo zigbuild --release --target <rust-triple>` (see the Makefile).
  * A vendored python-build-standalone tree at --target-python — populated by
    `scripts/bootstrap_target.py <platform> <arch>`.

No host tools are consulted (no ldd / otool / dumpbin, no host sysconfig).
Everything about the bundled Python runtime is read from --target-python,
which makes the output deterministic regardless of the developer's host.

macOS is host-only: targeting macOS from a non-macOS host exits with an error
because `install_name_tool` is needed and cannot be faked cross-platform.
"""

import argparse
import json
import platform as host_platform
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


# Stdlib directories excluded from the distribution — IDEs, test suites, and
# build tools that a shipped game does not need.
STDLIB_EXCLUDES = frozenset({
    "test",
    "tests",
    "idlelib",
    "tkinter",
    "turtledemo",
    "ensurepip",
    "__pycache__",
    "site-packages",
})


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
    p.add_argument("--rust-triple", required=True,
                   help="Rust target triple used for cargo (e.g. x86_64-pc-windows-msvc). "
                        "Determines target/<triple>/release/ binary location.")
    return p.parse_args()


# ── Target Python discovery ────────────────────────────────────────────────────

class TargetPython:
    """Describes the vendored python-build-standalone tree for one target."""

    def __init__(self, root: Path, target_platform: str):
        self.root = root                                  # .../vendor/python/<key>/python
        self.target_platform = target_platform            # linux|windows|macos
        self.version_short = self._detect_version()       # e.g. "3.12"

    def _detect_version(self) -> str:
        if self.target_platform == "windows":
            # python-build-standalone ships python3XX.dll at the install root.
            matches = sorted(self.root.glob("python3*.dll"))
            for m in matches:
                # Skip the "abi3 shim" python3.dll which has no minor version.
                mo = re.fullmatch(r"python3(\d+)\.dll", m.name)
                if mo:
                    return f"3.{mo.group(1)}"
            raise RuntimeError(f"Could not detect Python version in {self.root} (no python3XX.dll)")
        # unix
        matches = sorted(self.root.glob("lib/python3.*"))
        for m in matches:
            mo = re.fullmatch(r"python(3\.\d+)", m.name)
            if mo:
                return mo.group(1)
        raise RuntimeError(f"Could not detect Python version in {self.root} (no lib/python3.X/)")

    @property
    def stdlib_dir(self) -> Path:
        if self.target_platform == "windows":
            return self.root / "Lib"
        return self.root / "lib" / f"python{self.version_short}"

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
            # libpython is at lib/libpython3.X.dylib
            candidates = list((self.root / "lib").glob("libpython*.dylib"))
            if not candidates:
                raise RuntimeError(f"No libpython*.dylib in {self.root / 'lib'}")
            return candidates
        # linux
        candidates = list((self.root / "lib").glob("libpython*.so*"))
        if not candidates:
            raise RuntimeError(f"No libpython*.so* in {self.root / 'lib'}")
        return candidates

    @property
    def windows_dlls_dir(self) -> Path:
        return self.root / "DLLs"


# ── Runtime layout: copy libpython + stdlib + extension modules ───────────────

def install_runtime(
    tp: TargetPython,
    dest_binary: Path,
    dest_dir: Path,
) -> None:
    """Copy the target Python runtime into dest_dir and patch the binary if needed."""
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

    # Copy libpython — preserve symlinks so versioned + unversioned names both resolve.
    copied_names: list[str] = []
    for src in tp.libpython_files:
        dst = dest_python_lib / src.name
        if src.is_symlink():
            target = src.readlink()
            if dst.exists() or dst.is_symlink():
                dst.unlink()
            dst.symlink_to(target)
        else:
            shutil.copy2(src, dst)
        copied_names.append(src.name)
    print(f"         libpython: {', '.join(copied_names)}")

    # lib64 -> lib symlink for multiarch Python dlopen compatibility.
    lib64 = dest_dir / "python" / "lib64"
    if not lib64.exists():
        lib64.symlink_to("lib")

    # python-build-standalone ships unstripped libpython (~220 MB). Strip it
    # to shrink the distribution by ~200 MB with no runtime impact.
    _strip_libpython(dest_python_lib)

    _patch_rpath_linux(dest_binary, "$ORIGIN/python/lib")


def _strip_libpython(lib_dir: Path) -> None:
    # Strip the real (non-symlink) libpython binary. python-build-standalone's
    # Linux install_only tarballs include debug info which bloats the dist by
    # ~200 MB with no runtime benefit.
    target = next(
        (p for p in lib_dir.glob("libpython*.so*")
         if p.is_file() and not p.is_symlink()),
        None,
    )
    if target is None:
        return
    try:
        subprocess.run(["strip", "--strip-unneeded", str(target)], check=True)
        size_mb = target.stat().st_size // (1024 * 1024)
        print(f"         stripped {target.name} -> {size_mb} MB")
    except FileNotFoundError:
        print("  WARNING: `strip` not found; libpython kept unstripped.", file=sys.stderr)
    except subprocess.CalledProcessError as e:
        print(f"  WARNING: strip failed: {e}", file=sys.stderr)


def _install_runtime_macos(tp: TargetPython, dest_binary: Path, dest_dir: Path) -> None:
    if host_platform.system() != "Darwin":
        sys.exit(
            "ERROR: packaging a macOS target requires a macOS host.\n"
            "       `install_name_tool` is not portable across hosts — run this on a Mac."
        )
    dest_python_lib = dest_dir / "python" / "lib"
    dest_python_lib.mkdir(parents=True, exist_ok=True)

    dylib_src = tp.libpython_files[0]
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
    # but we don't have a built binary here until cargo-built it against the
    # same vendored dylib. Best-effort: discover the actual reference via
    # otool and rewrite it.
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
    for src in tp.libpython_files:
        shutil.copy2(src, dest_dir / src.name)
    print(f"         libpython: {', '.join(f.name for f in tp.libpython_files)}")

    # Extension modules (_ssl.pyd, _ctypes.pyd, …) live under DLLs/.
    dlls_src = tp.windows_dlls_dir
    if dlls_src.is_dir():
        shutil.copytree(
            dlls_src,
            dest_dir / "python" / "DLLs",
            dirs_exist_ok=True,
        )
        n = sum(1 for _ in dlls_src.iterdir())
        print(f"         DLLs/    : {n} extension modules")


def _patch_rpath_linux(binary: Path, rpath: str) -> None:
    try:
        subprocess.run(
            ["patchelf", "--set-rpath", rpath, str(binary)],
            check=True,
        )
        print(f"         RPATH set to: {rpath}")
    except FileNotFoundError:
        print(
            "  WARNING: patchelf not found — RPATH not patched.\n"
            "  The binary will only run if LD_LIBRARY_PATH includes python/lib/.\n"
            "  Install patchelf:  pacman -S patchelf  or  apt install patchelf",
            file=sys.stderr,
        )
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"patchelf failed: {e}") from e


# ── Python stdlib copy ─────────────────────────────────────────────────────────

def copy_stdlib(tp: TargetPython, dest_python: Path) -> None:
    """Copy the target Python stdlib into the layout the target interpreter expects."""
    stdlib_src = tp.stdlib_dir
    if not stdlib_src.is_dir():
        raise RuntimeError(f"stdlib not found at {stdlib_src}")

    if tp.target_platform == "windows":
        stdlib_dest = dest_python / "Lib"
        label = "Lib/"
    else:
        stdlib_dest = dest_python / "lib" / f"python{tp.version_short}"
        label = f"python{tp.version_short}/"
    stdlib_dest.mkdir(parents=True, exist_ok=True)

    copied = 0
    for item in stdlib_src.iterdir():
        if item.name in STDLIB_EXCLUDES:
            continue
        dest = stdlib_dest / item.name
        if item.is_dir():
            shutil.copytree(
                item, dest,
                ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "*.pyo"),
                dirs_exist_ok=True,
            )
        else:
            shutil.copy2(item, dest)
        copied += 1

    size_mb = sum(f.stat().st_size for f in stdlib_dest.rglob("*") if f.is_file()) // (1024 * 1024)
    print(f"         stdlib: {label} ({size_mb} MB, {copied} top-level entries)")


# ── Game bundle ────────────────────────────────────────────────────────────────

def create_bundle(game_dir: Path, dest_dir: Path) -> Path:
    """Zip game/**/*.py into dest_dir/game.bundle.

    Paths inside the zip are relative to game_dir's parent so that the module
    hierarchy (e.g. game.scripts.main) is preserved when the zip is on sys.path
    via Python's zipimport machinery.
    """
    bundle_path = dest_dir / "game.bundle"
    game_parent = game_dir.parent

    with zipfile.ZipFile(bundle_path, "w", compression=zipfile.ZIP_DEFLATED) as zf:
        for py_file in sorted(game_dir.rglob("*.py")):
            if "__pycache__" in py_file.parts:
                continue
            arcname = py_file.relative_to(game_parent)
            zf.write(py_file, arcname)

    size_kb = bundle_path.stat().st_size // 1024
    print(f"         bundle: game.bundle ({size_kb} KB)")
    return bundle_path


# ── Game data and project.json ────────────────────────────────────────────────

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


# ── Slugify helper ─────────────────────────────────────────────────────────────

def slugify(name: str) -> str:
    """Turn a game name into a filesystem-safe binary name (strip spaces)."""
    return "".join(c for c in name if c.isalnum() or c in "-_")


# ── Main ───────────────────────────────────────────────────────────────────────

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
    print("  [1/6] Locating release binary...")
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

    # ── 2. Install target Python runtime ─────────────────────────────────────
    print("  [2/6] Installing Python runtime...")
    install_runtime(tp, dest_binary, dest_dir)

    # ── 3. Copy Python stdlib ─────────────────────────────────────────────────
    print("  [3/6] Copying Python stdlib...")
    copy_stdlib(tp, dest_dir / "python")

    # ── 4. Create game.bundle ─────────────────────────────────────────────────
    print("  [4/6] Creating game.bundle...")
    create_bundle(game_dir, dest_dir)

    # ── 5. Copy project.json and game data ───────────────────────────────────
    print("  [5/6] Copying project.json and game data...")
    copy_project_json(game_dir, dest_dir)
    copy_game_data(game_dir, dest_dir)

    # ── 6. Summary ────────────────────────────────────────────────────────────
    total_mb = sum(f.stat().st_size for f in dest_dir.rglob("*") if f.is_file()) // (1024 * 1024)
    print()
    print(f"Done. Distribution size: {total_mb} MB")
    print(f"Launch:  {dest_dir / binary_name}")


if __name__ == "__main__":
    main()
