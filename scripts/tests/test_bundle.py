#!/usr/bin/env python3
"""Unit tests for scripts/bundle.py.

Most importantly, this file pins the **cross-language test vector** for the
canonical tree-hash algorithm — the Python side of the contract that
`crates/rython-cli/src/release_seal.rs::tree_hash_test_vector` enforces on
the Rust side. If either hex drifts, sealed release builds break.

Run standalone:
    python3 scripts/tests/test_bundle.py
"""

from __future__ import annotations

import hashlib
import os
import sys
import tempfile
import zipfile
from pathlib import Path

# Make `scripts/` importable when running this file directly.
sys.path.insert(0, str(Path(__file__).parent.parent))

import bundle  # noqa: E402


# ── Test vector (shared with Rust) ───────────────────────────────────────────

EXPECTED_TREE_HASH = "2b00599a262026e2bbde9ffc59a57a7a219e6ab6b5c6226f57f6862820b03736"


def test_tree_hash_vector():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "a.txt").write_bytes(b"alpha")
        (root / "sub").mkdir()
        (root / "sub" / "b.txt").write_bytes(b"beta")
        (root / "sub" / "c.txt").write_bytes(b"gamma")

        got = bundle.tree_hash(root)
        assert got == EXPECTED_TREE_HASH, (
            f"tree_hash drift — Rust side (release_seal::tree_hash_test_vector) "
            f"must match. Expected {EXPECTED_TREE_HASH}, got {got}"
        )


def test_tree_hash_empty_dir():
    with tempfile.TemporaryDirectory() as td:
        # Empty → outer sha256 with no updates → sha256("")
        assert bundle.tree_hash(Path(td)) == hashlib.sha256(b"").hexdigest()


def test_tree_hash_sort_is_bytewise():
    """File ordering must be bytewise, not locale-dependent. Regression guard."""
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "Z.txt").write_bytes(b"Z")
        (root / "a.txt").write_bytes(b"a")
        # Bytewise: 'Z' (0x5a) < 'a' (0x61), so Z comes first.
        outer = hashlib.sha256()
        for rel, content in [("Z.txt", b"Z"), ("a.txt", b"a")]:
            outer.update(rel.encode())
            outer.update(b"\x00")
            outer.update(hashlib.sha256(content).digest())
        assert bundle.tree_hash(root) == outer.hexdigest()


def test_sha256_file_matches_hashlib():
    with tempfile.TemporaryDirectory() as td:
        p = Path(td) / "f.bin"
        payload = b"\x00\x01\x02" * 10_000
        p.write_bytes(payload)
        assert bundle.sha256_file(p) == hashlib.sha256(payload).hexdigest()


def test_stdlib_zip_name_format():
    name = bundle.stdlib_zip_name()
    # e.g. "python313.zip"
    assert name.startswith("python"), name
    assert name.endswith(".zip"), name
    # No dot between version digits
    stem = name[len("python"):-len(".zip")]
    assert stem.isdigit(), f"expected digits only between 'python' and '.zip', got {stem!r}"


def test_build_game_bundle_produces_pyc_entries():
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        game = td_path / "game"
        (game / "scripts").mkdir(parents=True)
        (game / "__init__.py").write_text("")
        (game / "scripts" / "__init__.py").write_text("")
        (game / "scripts" / "main.py").write_text("def init():\n    pass\n")

        out = td_path / "out" / "game.bundle"
        bundle.build_game_bundle(game, out)
        assert out.exists()

        with zipfile.ZipFile(out) as zf:
            names = zf.namelist()
        # Everything should be .pyc, no .py files.
        assert names, "bundle should not be empty"
        assert all(n.endswith(".pyc") for n in names), f"non-pyc entries: {names}"
        # The entry-point path must be present.
        assert "game/scripts/main.pyc" in names, names


# ── .pth guard ───────────────────────────────────────────────────────────────

def test_assert_no_pth_files_passes_on_clean_tree():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "a.so").write_bytes(b"ext")
        (root / "sub").mkdir()
        (root / "sub" / "b.so").write_bytes(b"ext")
        # Should not raise / SystemExit.
        bundle.assert_no_pth_files(root, "fixture")


def test_assert_no_pth_files_rejects_offender():
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        (root / "evil.pth").write_text("import os\n")
        raised = False
        try:
            bundle.assert_no_pth_files(root, "fixture")
        except SystemExit:
            raised = True
        assert raised, "assert_no_pth_files must fail loudly on a .pth file"


# ── Symlink handling ─────────────────────────────────────────────────────────

def test_tree_hash_skips_symlinked_directory():
    """Rust's `collect_files` skips symlinks (they report neither
    `is_file()` nor `is_dir()` on the dirent). The Python side must match.
    """
    if not hasattr(os, "symlink"):
        return  # Windows without privilege — skip
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        root = td_path / "root"
        root.mkdir()
        (root / "plain.txt").write_bytes(b"payload")

        sibling = td_path / "sibling"
        sibling.mkdir()
        (sibling / "hidden.txt").write_bytes(b"not-in-tree")

        try:
            os.symlink(sibling, root / "symlinked-dir", target_is_directory=True)
        except (OSError, NotImplementedError):
            return  # no symlink privilege

        got = bundle.tree_hash(root)
        # Should equal hashing a tree with only `plain.txt`.
        with tempfile.TemporaryDirectory() as td2:
            plain = Path(td2)
            (plain / "plain.txt").write_bytes(b"payload")
            expected = bundle.tree_hash(plain)
        assert got == expected, (
            "tree_hash followed a symlinked directory — Rust side does not, "
            "so cross-language hashes will drift."
        )


def test_tree_hash_skips_symlinked_file():
    if not hasattr(os, "symlink"):
        return
    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        root = td_path / "root"
        root.mkdir()
        (root / "real.txt").write_bytes(b"payload")

        target = td_path / "external.txt"
        target.write_bytes(b"not-in-tree")

        try:
            os.symlink(target, root / "link.txt")
        except (OSError, NotImplementedError):
            return

        got = bundle.tree_hash(root)
        # Should equal hashing a tree with only `real.txt`.
        with tempfile.TemporaryDirectory() as td2:
            plain = Path(td2)
            (plain / "real.txt").write_bytes(b"payload")
            expected = bundle.tree_hash(plain)
        assert got == expected


# ── Driver ───────────────────────────────────────────────────────────────────

def main() -> int:
    tests = [
        ("test_tree_hash_vector", test_tree_hash_vector),
        ("test_tree_hash_empty_dir", test_tree_hash_empty_dir),
        ("test_tree_hash_sort_is_bytewise", test_tree_hash_sort_is_bytewise),
        ("test_tree_hash_skips_symlinked_directory",
         test_tree_hash_skips_symlinked_directory),
        ("test_tree_hash_skips_symlinked_file",
         test_tree_hash_skips_symlinked_file),
        ("test_sha256_file_matches_hashlib", test_sha256_file_matches_hashlib),
        ("test_stdlib_zip_name_format", test_stdlib_zip_name_format),
        ("test_build_game_bundle_produces_pyc_entries",
         test_build_game_bundle_produces_pyc_entries),
        ("test_assert_no_pth_files_passes_on_clean_tree",
         test_assert_no_pth_files_passes_on_clean_tree),
        ("test_assert_no_pth_files_rejects_offender",
         test_assert_no_pth_files_rejects_offender),
    ]
    failed = 0
    for name, fn in tests:
        try:
            fn()
            print(f"  PASS  {name}")
        except Exception as e:
            print(f"  FAIL  {name}: {e}")
            failed += 1
    print()
    print(f"{len(tests) - failed}/{len(tests)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
