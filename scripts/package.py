#!/usr/bin/env python3
"""scripts/package.py — Assemble a self-contained RythonEngine release distribution.

Usage:
    python3 scripts/package.py --platform linux --arch x86_64 --game game --out dist/linux-x86_64
    python3 scripts/package.py --platform windows --arch x86_64 --game game --out dist/windows-x86_64
    python3 scripts/package.py --platform macos --arch aarch64 --game game --out dist/macos-aarch64

The script expects `cargo build --release` to have already run (make dist handles this).
"""

import argparse
import json
import os
import platform
import shutil
import subprocess
import sys
import sysconfig
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
    return p.parse_args()


# ── libpython detection ────────────────────────────────────────────────────────

def detect_libpython(binary: Path, target_platform: str) -> tuple[Path, str]:
    """Return (resolved_path, soname) for the libpython linked into binary."""
    if target_platform == "linux":
        return _detect_libpython_linux(binary)
    elif target_platform == "macos":
        return _detect_libpython_macos(binary)
    elif target_platform == "windows":
        return _detect_libpython_windows(binary)
    raise ValueError(f"Unsupported platform: {target_platform}")


def _detect_libpython_linux(binary: Path) -> tuple[Path, str]:
    try:
        out = subprocess.check_output(["ldd", str(binary)], text=True, stderr=subprocess.DEVNULL)
        for line in out.splitlines():
            line = line.strip()
            if "libpython" not in line:
                continue
            parts = line.split()
            soname = parts[0]
            if "=>" in parts:
                idx = parts.index("=>")
                if idx + 1 < len(parts) and parts[idx + 1] != "not":
                    resolved = Path(parts[idx + 1])
                    if resolved.exists():
                        return resolved, soname
    except (subprocess.CalledProcessError, FileNotFoundError):
        pass

    # Fall back to sysconfig
    libdir = Path(sysconfig.get_config_var("LIBDIR") or "")
    instsoname = sysconfig.get_config_var("INSTSONAME") or ""
    candidate = libdir / instsoname
    if candidate.exists():
        return candidate, instsoname

    raise RuntimeError(
        "Could not locate libpython. "
        "Ensure the Python development libraries are installed."
    )


def _detect_libpython_macos(binary: Path) -> tuple[Path, str]:
    out = subprocess.check_output(["otool", "-L", str(binary)], text=True)
    for line in out.splitlines():
        stripped = line.strip()
        if "libpython" in stripped or ("/Python" in stripped and "framework" not in stripped.lower()):
            dylib_path = Path(stripped.split()[0])
            return dylib_path, dylib_path.name
    raise RuntimeError("Could not detect libpython via otool -L")


def _detect_libpython_windows(binary: Path) -> tuple[Path, str]:
    try:
        out = subprocess.check_output(
            ["dumpbin", "/dependents", str(binary)], text=True, stderr=subprocess.DEVNULL
        )
    except FileNotFoundError:
        raise RuntimeError(
            "dumpbin not found. Run this script from a Visual Studio Developer Command Prompt."
        )
    for line in out.splitlines():
        name = line.strip()
        if name.lower().startswith("python") and name.lower().endswith(".dll"):
            # Search common locations
            prefix = Path(sys.prefix)
            candidates = [
                prefix / name,
                prefix / "DLLs" / name,
                Path(os.environ.get("SYSTEMROOT", r"C:\Windows")) / "System32" / name,
            ]
            for c in candidates:
                if c.exists():
                    return c, name
            raise RuntimeError(
                f"Found {name} in dumpbin but could not locate the file. "
                "Ensure Python is installed and on PATH."
            )
    raise RuntimeError("Could not detect python DLL via dumpbin")


# ── RPATH / install-name patching ─────────────────────────────────────────────

def patch_binary_rpath(
    binary: Path,
    libpython_src: Path,
    soname: str,
    dest_dir: Path,
    target_platform: str,
) -> None:
    """Copy libpython into the dist tree and patch the binary to find it."""
    dest_python_lib = dest_dir / "python" / "lib"
    dest_python_lib.mkdir(parents=True, exist_ok=True)

    if target_platform == "linux":
        dest_so = dest_python_lib / soname
        # Resolve symlinks to get the real versioned file
        real_src = libpython_src.resolve()
        shutil.copy2(real_src, dest_so)

        # lib64 -> lib symlink for multiarch Python dlopen compatibility
        lib64 = dest_dir / "python" / "lib64"
        if not lib64.exists():
            lib64.symlink_to("lib")

        _patch_rpath_linux(binary, "$ORIGIN/python/lib")

    elif target_platform == "macos":
        dest_dylib = dest_python_lib / soname
        shutil.copy2(libpython_src, dest_dylib)
        _patch_rpath_macos(binary, str(libpython_src), soname)

    elif target_platform == "windows":
        # On Windows the DLL is resolved from the exe's directory
        shutil.copy2(libpython_src, dest_dir / soname)
        # Also copy the DLLs directory (extension modules like _ssl.pyd, _ctypes.pyd)
        py_dlls_src = Path(sys.prefix) / "DLLs"
        if py_dlls_src.is_dir():
            shutil.copytree(
                py_dlls_src,
                dest_dir / "python" / "DLLs",
                dirs_exist_ok=True,
            )


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


def _patch_rpath_macos(binary: Path, old_dylib_path: str, soname: str) -> None:
    dest_dylib = binary.parent / "python" / "lib" / soname
    # Set the dylib's own install name
    subprocess.run(
        ["install_name_tool", "-id", f"@loader_path/{soname}", str(dest_dylib)],
        check=True,
    )
    # Rewrite the binary's reference to point at the bundled copy
    subprocess.run(
        ["install_name_tool", "-change",
         old_dylib_path,
         f"@executable_path/python/lib/{soname}",
         str(binary)],
        check=True,
    )


# ── Python stdlib copy ─────────────────────────────────────────────────────────

def copy_stdlib(dest_python: Path) -> None:
    """Copy the Python stdlib into dest_python/lib/pythonX.Y/, minus excluded dirs."""
    stdlib_src = Path(sysconfig.get_paths()["stdlib"])
    py_ver = sysconfig.get_python_version()  # e.g. "3.14"
    stdlib_dest = dest_python / "lib" / f"python{py_ver}"
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
    print(f"         stdlib: python{py_ver}/ ({size_mb} MB, {copied} top-level entries)")


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


# ── Assets and project.json ────────────────────────────────────────────────────

def copy_assets(game_dir: Path, dest_dir: Path) -> None:
    assets_src = game_dir / "assets"
    if not assets_src.is_dir():
        print(f"  WARNING: no assets/ directory found at {assets_src}")
        return
    shutil.copytree(assets_src, dest_dir / "assets", dirs_exist_ok=True)
    n = sum(1 for _ in (dest_dir / "assets").rglob("*") if _.is_file())
    print(f"         assets: {n} files")


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

    print(f"Packaging  {game_name}  →  {args.platform}-{args.arch}")
    print(f"Output:    {dest_dir}")
    print()

    # ── 1. Locate the release binary ──────────────────────────────────────────
    print("  [1/6] Locating release binary...")
    src_binary_name = "rython.exe" if args.platform == "windows" else "rython"
    binary_src = repo_root / "target" / "release" / src_binary_name
    if not binary_src.exists():
        sys.exit(
            f"ERROR: release binary not found at {binary_src}\n"
            "Run `make release` first, or use `make dist` which builds automatically."
        )
    print(f"         found: {binary_src}")

    dest_binary = dest_dir / binary_name
    shutil.copy2(binary_src, dest_binary)
    if args.platform != "windows":
        dest_binary.chmod(dest_binary.stat().st_mode | 0o111)

    # ── 2. Detect libpython ───────────────────────────────────────────────────
    print("  [2/6] Detecting libpython...")
    libpython_src, soname = detect_libpython(binary_src, args.platform)
    print(f"         found: {libpython_src}  ({soname})")

    # ── 3. Copy libpython + patch RPATH ──────────────────────────────────────
    print("  [3/6] Copying runtime and patching RPATH...")
    patch_binary_rpath(dest_binary, libpython_src, soname, dest_dir, args.platform)

    # ── 4. Copy Python stdlib ─────────────────────────────────────────────────
    print("  [4/6] Copying Python stdlib...")
    copy_stdlib(dest_dir / "python")

    # ── 5. Create game.bundle ─────────────────────────────────────────────────
    print("  [5/6] Creating game.bundle...")
    create_bundle(game_dir, dest_dir)

    # ── 6. Copy project.json and assets ──────────────────────────────────────
    print("  [6/6] Copying project.json and assets...")
    copy_project_json(game_dir, dest_dir)
    copy_assets(game_dir, dest_dir)

    total_mb = sum(f.stat().st_size for f in dest_dir.rglob("*") if f.is_file()) // (1024 * 1024)
    print()
    print(f"Done. Distribution size: {total_mb} MB")
    print(f"Launch:  {dest_dir / binary_name}")


if __name__ == "__main__":
    main()
