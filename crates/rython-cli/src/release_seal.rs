//! Release-seal integrity verification.
//!
//! Release binaries ship with SHA-256 digests of every file they will load
//! (game bundle, pre-compiled stdlib zip, and `lib-dynload` binary
//! extensions) baked in as compile-time constants. Before the Python
//! interpreter is ever touched, `verify()` recomputes those digests from the
//! on-disk distribution and refuses to boot if any of them have changed.
//!
//! Verification is implemented in pure Rust (`sha2` crate) because CPython's
//! `hashlib` lives inside the *unverified* stdlib — we cannot use it to
//! verify itself.
//!
//! # Tree hash algorithm (shared with `scripts/bundle.py`)
//!
//! For the `lib-dynload` / `DLLs` directory we need a canonical hash over
//! a directory tree. The algorithm is byte-identical in Rust and Python and
//! MUST stay that way — drift breaks release-mode launches.
//!
//! 1. Walk the tree, collecting every regular file.
//! 2. Sort by forward-slash relative path, sort order = bytewise ascending.
//! 3. For each file in that order, feed the following bytes into a single
//!    outer SHA-256 hasher:
//!      - `relative_path.as_bytes()`
//!      - one NUL byte (`0x00`)
//!      - `sha256(file_contents)` (32 raw bytes, not hex)
//! 4. The tree hash is the hex digest of the outer hasher.
//!
//! A shared test vector (`tree_hash_test_vector`) pins this contract.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

// ── Compile-time constants (forwarded by build.rs) ───────────────────────────

pub const BUNDLE_HASH: Option<&str> = option_env!("RYTHON_BUNDLE_HASH");
pub const STDLIB_HASH: Option<&str> = option_env!("RYTHON_STDLIB_HASH");
pub const LIBDYNLOAD_HASH: Option<&str> = option_env!("RYTHON_LIBDYNLOAD_HASH");
pub const STDLIB_ZIP_NAME: Option<&str> = option_env!("RYTHON_STDLIB_ZIP_NAME");
pub const ENTRY_POINT: Option<&str> = option_env!("RYTHON_ENTRY_POINT");
pub const SEALED: Option<&str> = option_env!("RYTHON_SEALED");

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum SealError {
    #[error(
        "release binary was built without integrity seals (RYTHON_SEALED=1 not set at compile time); \
         refusing to enter release mode"
    )]
    Unsealed,
    #[error("game.bundle integrity check failed: expected {expected}, got {actual}")]
    BundleMismatch { expected: String, actual: String },
    #[error("stdlib zip integrity check failed: expected {expected}, got {actual}")]
    StdlibMismatch { expected: String, actual: String },
    #[error("lib-dynload tree integrity check failed: expected {expected}, got {actual}")]
    LibDynloadMismatch { expected: String, actual: String },
    #[error("failed to read {path}: {source}", path = path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

// ── Verified seal ────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct VerifiedSeal {
    pub bundle_path: PathBuf,
    pub entry_point: String,
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Verify that the distribution at `proj_dir` matches the hashes baked into
/// this binary at compile time.
///
/// Returns a `VerifiedSeal` whose `entry_point` is the compile-time constant —
/// the runtime `project.json` field is ignored in release mode.
pub fn verify(proj_dir: &Path) -> Result<VerifiedSeal, SealError> {
    let (bundle_expected, stdlib_expected, libdyn_expected, zip_name, entry_point) =
        sealed_constants().ok_or(SealError::Unsealed)?;
    verify_inner(
        proj_dir,
        bundle_expected,
        stdlib_expected,
        libdyn_expected,
        zip_name,
        entry_point,
    )
}

/// Inner verification with explicit expected values — extracted so tests can
/// exercise the full hash-and-compare path without needing the compile-time
/// constants to be set.
fn verify_inner(
    proj_dir: &Path,
    bundle_expected: &str,
    stdlib_expected: &str,
    libdyn_expected: &str,
    zip_name: &str,
    entry_point: &str,
) -> Result<VerifiedSeal, SealError> {
    // 1. game.bundle
    let bundle_path = proj_dir.join("game.bundle");
    let bundle_actual = hex_sha256_file(&bundle_path)?;
    if !const_time_eq(&bundle_actual, bundle_expected) {
        return Err(SealError::BundleMismatch {
            expected: bundle_expected.to_string(),
            actual: bundle_actual,
        });
    }

    // 2. stdlib zip (pythonX.Y.zip)
    let stdlib_zip_path = stdlib_zip_path(proj_dir, zip_name);
    let stdlib_actual = hex_sha256_file(&stdlib_zip_path)?;
    if !const_time_eq(&stdlib_actual, stdlib_expected) {
        return Err(SealError::StdlibMismatch {
            expected: stdlib_expected.to_string(),
            actual: stdlib_actual,
        });
    }

    // 3. lib-dynload / DLLs tree
    let dynload_dir = lib_dynload_path(proj_dir, zip_name);
    let libdyn_actual = tree_hash(&dynload_dir)?;
    if !const_time_eq(&libdyn_actual, libdyn_expected) {
        return Err(SealError::LibDynloadMismatch {
            expected: libdyn_expected.to_string(),
            actual: libdyn_actual,
        });
    }

    Ok(VerifiedSeal {
        bundle_path,
        entry_point: entry_point.to_string(),
    })
}

/// Short prefix for info-level logging — aids forensics without leaking
/// full digests into log aggregators.
pub fn short_hex(hex: &str) -> &str {
    &hex[..hex.len().min(12)]
}

// ── Constants gate ───────────────────────────────────────────────────────────

/// Returns `Some((bundle, stdlib, libdyn, zip_name, entry_point))` only when
/// every required compile-time constant is present *and* `RYTHON_SEALED=1`.
fn sealed_constants() -> Option<(&'static str, &'static str, &'static str, &'static str, &'static str)> {
    if SEALED? != "1" {
        return None;
    }
    Some((
        BUNDLE_HASH?,
        STDLIB_HASH?,
        LIBDYNLOAD_HASH?,
        STDLIB_ZIP_NAME?,
        ENTRY_POINT?,
    ))
}

// ── Path helpers ─────────────────────────────────────────────────────────────

fn stdlib_zip_path(proj_dir: &Path, zip_name: &str) -> PathBuf {
    proj_dir.join("python").join("lib").join(zip_name)
}

/// POSIX: `python/lib/pythonX.Y/lib-dynload`
/// Windows: `python/DLLs`
///
/// `zip_name` is e.g. `python313.zip`; we derive the `pythonX.Y/` subdir from it.
fn lib_dynload_path(proj_dir: &Path, zip_name: &str) -> PathBuf {
    if cfg!(windows) {
        proj_dir.join("python").join("DLLs")
    } else {
        proj_dir
            .join("python")
            .join("lib")
            .join(python_xy_from_zip_name(zip_name))
            .join("lib-dynload")
    }
}

/// Convert `python313.zip` → `python3.13`. Fallback passes through on any
/// unexpected format; mismatched path surfaces as a missing-file IO error
/// rather than a silent wrong-directory scan.
fn python_xy_from_zip_name(zip_name: &str) -> String {
    let stem = zip_name.strip_suffix(".zip").unwrap_or(zip_name);
    let digits: String = stem.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 2 {
        let (major, minor) = digits.split_at(1);
        format!("python{major}.{minor}")
    } else {
        stem.to_string()
    }
}

// ── Hashing primitives ───────────────────────────────────────────────────────

fn hex_sha256_file(path: &Path) -> Result<String, SealError> {
    let mut file = fs::File::open(path).map_err(|e| SealError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|e| SealError::Io {
        path: path.to_path_buf(),
        source: e,
    })?;
    Ok(hex(&hasher.finalize()))
}

fn hex_sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Canonical tree hash. See module docs for the algorithm.
pub fn tree_hash(root: &Path) -> Result<String, SealError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    let mut outer = Sha256::new();
    for (rel, abs) in &files {
        let bytes = fs::read(abs).map_err(|e| SealError::Io {
            path: abs.clone(),
            source: e,
        })?;
        let file_digest = hex_sha256_bytes(&bytes);
        outer.update(rel.as_bytes());
        outer.update([0x00]);
        outer.update(file_digest);
    }
    Ok(hex(&outer.finalize()))
}

fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, PathBuf)>,
) -> Result<(), SealError> {
    let entries = fs::read_dir(dir).map_err(|e| SealError::Io {
        path: dir.to_path_buf(),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| SealError::Io {
            path: dir.to_path_buf(),
            source: e,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| SealError::Io {
            path: path.clone(),
            source: e,
        })?;
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|_| SealError::Io {
                    path: path.clone(),
                    source: io::Error::new(io::ErrorKind::Other, "relative path outside tree root"),
                })?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// Constant-time byte-equality on the common prefix, plus a length diff in
/// the accumulator so unequal lengths never short-circuit early. All
/// comparisons in this module are SHA-256 hex digests (always 64 bytes), so
/// the length branch is effectively dead — but keeping it branchless avoids
/// future-footgun surprises if a caller passes truncated or uppercase hex.
fn const_time_eq(a: &str, b: &str) -> bool {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());
    let len_diff = (ab.len() ^ bb.len()) as u32;
    let mut diff: u32 = len_diff;
    let common = ab.len().min(bb.len());
    for i in 0..common {
        diff |= (ab[i] ^ bb[i]) as u32;
    }
    diff == 0
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(path).unwrap();
        f.write_all(bytes).unwrap();
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn hex_sha256_empty_string() {
        let digest = hex(&hex_sha256_bytes(b""));
        assert_eq!(
            digest,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    /// **Cross-language test vector.** The same fixture is hashed in
    /// `scripts/tests/test_bundle.py` and must produce the identical hex
    /// digest. If either side drifts the vector, release builds break.
    #[test]
    fn tree_hash_test_vector() {
        let dir = tmp();
        write(&dir.path().join("a.txt"), b"alpha");
        write(&dir.path().join("sub/b.txt"), b"beta");
        write(&dir.path().join("sub/c.txt"), b"gamma");

        let got = tree_hash(dir.path()).unwrap();
        assert_eq!(
            got,
            "2b00599a262026e2bbde9ffc59a57a7a219e6ab6b5c6226f57f6862820b03736",
            "tree_hash drift — Python side must match"
        );
    }

    #[test]
    fn const_time_eq_matches() {
        assert!(const_time_eq("abc", "abc"));
        assert!(!const_time_eq("abc", "abd"));
        assert!(!const_time_eq("abc", "abcd"));
    }

    #[test]
    fn python_xy_derivation() {
        assert_eq!(python_xy_from_zip_name("python313.zip"), "python3.13");
        assert_eq!(python_xy_from_zip_name("python314.zip"), "python3.14");
    }

    // ── End-to-end verify_inner tests ────────────────────────────────────────

    const ZIP_NAME: &str = "python313.zip";
    const ENTRY: &str = "game.scripts.main";

    /// Build a minimal sealed-dist layout under `root` and return the three
    /// expected hex digests (bundle, stdlib, libdyn).
    fn build_fixture(root: &Path) -> (String, String, String) {
        let bundle = root.join("game.bundle");
        write(&bundle, b"fake-bundle-bytes");

        let stdlib_zip = root.join("python").join("lib").join(ZIP_NAME);
        write(&stdlib_zip, b"fake-stdlib-zip");

        let dynload = root
            .join("python")
            .join("lib")
            .join("python3.13")
            .join("lib-dynload");
        write(&dynload.join("_ssl.so"), b"ssl-ext");
        write(&dynload.join("sub/_hashlib.so"), b"hashlib-ext");

        let bundle_hex = hex_sha256_file(&bundle).unwrap();
        let stdlib_hex = hex_sha256_file(&stdlib_zip).unwrap();
        let libdyn_hex = tree_hash(&dynload).unwrap();
        (bundle_hex, stdlib_hex, libdyn_hex)
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_happy_path() {
        let dir = tmp();
        let (b, s, l) = build_fixture(dir.path());
        let seal = verify_inner(dir.path(), &b, &s, &l, ZIP_NAME, ENTRY).unwrap();
        assert_eq!(seal.entry_point, ENTRY);
        assert_eq!(seal.bundle_path, dir.path().join("game.bundle"));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_bundle_mismatch() {
        let dir = tmp();
        let (_, s, l) = build_fixture(dir.path());
        let bad = "0".repeat(64);
        let err = verify_inner(dir.path(), &bad, &s, &l, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::BundleMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_stdlib_mismatch() {
        let dir = tmp();
        let (b, _, l) = build_fixture(dir.path());
        let bad = "f".repeat(64);
        let err = verify_inner(dir.path(), &b, &bad, &l, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::StdlibMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_libdynload_mismatch() {
        let dir = tmp();
        let (b, s, _) = build_fixture(dir.path());
        let bad = "a".repeat(64);
        let err = verify_inner(dir.path(), &b, &s, &bad, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibDynloadMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_surfaces_io_error_on_missing_bundle() {
        let dir = tmp();
        let err = verify_inner(
            dir.path(),
            &"0".repeat(64),
            &"0".repeat(64),
            &"0".repeat(64),
            ZIP_NAME,
            ENTRY,
        )
        .unwrap_err();
        assert!(matches!(err, SealError::Io { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_tampered_libdynload_file() {
        let dir = tmp();
        let (b, s, l) = build_fixture(dir.path());
        // Flip one byte in a lib-dynload file
        let tampered = dir.path().join("python/lib/python3.13/lib-dynload/_ssl.so");
        write(&tampered, b"TAMPERED");
        let err = verify_inner(dir.path(), &b, &s, &l, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibDynloadMismatch { .. }));
    }

    #[test]
    fn short_hex_truncates_to_twelve() {
        assert_eq!(short_hex("0123456789abcdef"), "0123456789ab");
        assert_eq!(short_hex("abc"), "abc");
    }
}
