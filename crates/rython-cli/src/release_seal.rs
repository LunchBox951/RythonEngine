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
pub const LIBPYTHON_HASH: Option<&str> = option_env!("RYTHON_LIBPYTHON_HASH");
pub const LIBPYTHON_SONAME: Option<&str> = option_env!("RYTHON_LIBPYTHON_SONAME");
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
    #[error("libpython integrity check failed: expected {expected}, got {actual}")]
    LibpythonMismatch { expected: String, actual: String },
    #[error("libpython shared object not found at {}", path.display())]
    LibpythonNotFound { path: PathBuf },
    #[error(
        "unexpected path found in sealed distribution at {}: \
         this location is not shipped with the release and its presence suggests tampering",
        path.display()
    )]
    UnexpectedPath { path: PathBuf },
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
    let (
        bundle_expected,
        stdlib_expected,
        libdyn_expected,
        libpython_expected,
        libpython_soname,
        zip_name,
        entry_point,
    ) = sealed_constants().ok_or(SealError::Unsealed)?;
    verify_inner(
        proj_dir,
        bundle_expected,
        stdlib_expected,
        libdyn_expected,
        libpython_expected,
        libpython_soname,
        zip_name,
        entry_point,
    )
}

/// Inner verification with explicit expected values — extracted so tests can
/// exercise the full hash-and-compare path without needing the compile-time
/// constants to be set.
#[allow(clippy::too_many_arguments)]
fn verify_inner(
    proj_dir: &Path,
    bundle_expected: &str,
    stdlib_expected: &str,
    libdyn_expected: &str,
    libpython_expected: &str,
    libpython_soname: &str,
    zip_name: &str,
    entry_point: &str,
) -> Result<VerifiedSeal, SealError> {
    // 1. libpython — checked first because the dynamic linker resolves this
    //    file before `main()` runs. We cannot prevent a tampered libpython's
    //    constructor from firing, but we can guarantee the sealed binary
    //    refuses to continue past this point, denying the attacker a
    //    Python-side beachhead.
    //
    //    No separate `exists()` check: that would open a TOCTOU window
    //    between the stat call and the file open. Instead, `hex_sha256_file`
    //    opens the file once and we map `NotFound` to the friendly error
    //    inline — the hash is computed on whatever bytes the open returns,
    //    so a swap between the two syscalls can no longer yield the
    //    legitimate file's existence signal with the attacker's bytes.
    let libpython_path = libpython_path(proj_dir, libpython_soname);
    let libpython_actual = match hex_sha256_file(&libpython_path) {
        Ok(hex) => hex,
        Err(SealError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            return Err(SealError::LibpythonNotFound {
                path: libpython_path,
            });
        }
        Err(e) => return Err(e),
    };
    if !const_time_eq(&libpython_actual, libpython_expected) {
        return Err(SealError::LibpythonMismatch {
            expected: libpython_expected.to_string(),
            actual: libpython_actual,
        });
    }

    // 2. game.bundle
    let bundle_path = proj_dir.join("game.bundle");
    let bundle_actual = hex_sha256_file(&bundle_path)?;
    if !const_time_eq(&bundle_actual, bundle_expected) {
        return Err(SealError::BundleMismatch {
            expected: bundle_expected.to_string(),
            actual: bundle_actual,
        });
    }

    // 3. stdlib zip (pythonX.Y.zip)
    let stdlib_zip_path = stdlib_zip_path(proj_dir, zip_name);
    let stdlib_actual = hex_sha256_file(&stdlib_zip_path)?;
    if !const_time_eq(&stdlib_actual, stdlib_expected) {
        return Err(SealError::StdlibMismatch {
            expected: stdlib_expected.to_string(),
            actual: stdlib_actual,
        });
    }

    // 4. lib-dynload / DLLs tree
    let dynload_dir = lib_dynload_path(proj_dir, zip_name);
    let libdyn_actual = tree_hash(&dynload_dir)?;
    if !const_time_eq(&libdyn_actual, libdyn_expected) {
        return Err(SealError::LibDynloadMismatch {
            expected: libdyn_expected.to_string(),
            actual: libdyn_actual,
        });
    }

    // 5. Absent-by-design paths — directories `site.py` scans for `.pth`
    //    files at interpreter startup. We never ship any of them, so
    //    presence at runtime indicates post-install tampering aimed at
    //    sys.path injection: the pivot a write-access-to-install-dir
    //    attacker reaches for after phase 1 blocks the bytecode path.
    //
    //    Layout differs by platform:
    //      POSIX:   `python/lib/pythonX.Y/{site-packages,dist-packages}`
    //               (canonical + Debian/Ubuntu convention)
    //      Windows: `python/Lib/site-packages`
    //               (`site.py`'s Windows branch resolves site-packages
    //                relative to `sys.prefix/Lib`, not next to DLLs)
    for p in absent_by_design_paths(proj_dir, &dynload_dir) {
        if p.exists() {
            return Err(SealError::UnexpectedPath { path: p });
        }
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

/// Returns the full sealed-constants tuple only when every required
/// compile-time constant is present *and* `RYTHON_SEALED=1`.
///
/// Tuple order: `(bundle, stdlib, libdyn, libpython, libpython_soname,
/// zip_name, entry_point)`.
fn sealed_constants() -> Option<(
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
)> {
    if SEALED? != "1" {
        return None;
    }
    Some((
        BUNDLE_HASH?,
        STDLIB_HASH?,
        LIBDYNLOAD_HASH?,
        LIBPYTHON_HASH?,
        LIBPYTHON_SONAME?,
        STDLIB_ZIP_NAME?,
        ENTRY_POINT?,
    ))
}

// ── Path helpers ─────────────────────────────────────────────────────────────

fn stdlib_zip_path(proj_dir: &Path, zip_name: &str) -> PathBuf {
    proj_dir.join("python").join("lib").join(zip_name)
}

/// POSIX: `python/lib/<soname>` (e.g. `libpython3.13.so.1.0`).
/// Windows: `<soname>` in the dist root (e.g. `python313.dll`) — matches
/// `scripts/package.py:patch_binary_rpath`'s placement of the DLL alongside
/// the executable so the OS loader resolves it without a per-user PATH entry.
fn libpython_path(proj_dir: &Path, soname: &str) -> PathBuf {
    if cfg!(windows) {
        proj_dir.join(soname)
    } else {
        proj_dir.join("python").join("lib").join(soname)
    }
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

/// Paths that must not exist in a sealed dist — all are `site.py` scan
/// targets for `.pth` files, which can inject arbitrary entries into
/// `sys.path` at interpreter startup. Each platform's layout surfaces a
/// different canonical set:
///
/// POSIX:
///   `<lib-dynload parent>/site-packages`  (canonical)
///   `<lib-dynload parent>/dist-packages`  (Debian/Ubuntu)
///
/// Windows:
///   `<proj>/python/Lib/site-packages`     (CPython's `site.getsitepackages`
///                                          Windows branch — not adjacent to
///                                          `DLLs/`)
fn absent_by_design_paths(proj_dir: &Path, dynload_dir: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![proj_dir.join("python").join("Lib").join("site-packages")]
    } else {
        match dynload_dir.parent() {
            Some(parent) => vec![parent.join("site-packages"), parent.join("dist-packages")],
            None => Vec::new(),
        }
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
        if file_type.is_symlink() {
            // Reject symlinks explicitly rather than silently skip them.
            // `bundle.py::assert_no_symlinks` guarantees the *built* tree is
            // symlink-free, so any symlink observed here at runtime is a
            // post-install injection: an attacker with write access to the
            // dist can drop `_evil.so -> /tmp/evil.so` alongside the real
            // extensions; since tree_hash would previously skip it, the hash
            // would still match and `import ssl` (or any shadowed module name)
            // would load the attacker's payload. Erroring closes that gap.
            return Err(SealError::UnexpectedPath { path });
        }
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let rel_path = path.strip_prefix(root).map_err(|_| SealError::Io {
                path: path.clone(),
                source: io::Error::other("relative path outside tree root"),
            })?;
            // Explicit UTF-8 requirement — must match Python's `as_posix()`
            // byte-for-byte or the cross-language hash contract silently
            // drifts. `to_string_lossy()` would replace invalid bytes with
            // U+FFFD, which Python doesn't do, producing a hash the runtime
            // can't reproduce and a sealed build that won't boot.
            let rel = rel_path
                .to_str()
                .ok_or_else(|| SealError::Io {
                    path: path.clone(),
                    source: io::Error::new(
                        io::ErrorKind::InvalidData,
                        "non-UTF-8 filename in sealed tree",
                    ),
                })?
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
            got, "2b00599a262026e2bbde9ffc59a57a7a219e6ab6b5c6226f57f6862820b03736",
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
    //
    // The fixture below models a POSIX-layout sealed dist (lib-dynload under
    // python/lib/pythonX.Y/). Windows uses python/DLLs/ instead — mirroring
    // that layout end-to-end is a separate test surface we don't cover here,
    // so the whole fixture + its verify_inner_* tests are cfg(not(windows)).
    // The `absent_by_design_paths_layout` test below exercises the Windows
    // branch of the path logic without needing a full fixture.

    #[cfg(not(windows))]
    const ZIP_NAME: &str = "python313.zip";
    #[cfg(not(windows))]
    const ENTRY: &str = "game.scripts.main";
    #[cfg(not(windows))]
    const SONAME: &str = "libpython3.13.so.1.0";

    /// Build a minimal sealed-dist layout under `root` and return the four
    /// expected hex digests (bundle, stdlib, libdyn, libpython).
    #[cfg(not(windows))]
    fn build_fixture(root: &Path) -> (String, String, String, String) {
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

        let libpython = root.join("python").join("lib").join(SONAME);
        write(&libpython, b"fake-libpython-bytes");

        let bundle_hex = hex_sha256_file(&bundle).unwrap();
        let stdlib_hex = hex_sha256_file(&stdlib_zip).unwrap();
        let libdyn_hex = tree_hash(&dynload).unwrap();
        let libpython_hex = hex_sha256_file(&libpython).unwrap();
        (bundle_hex, stdlib_hex, libdyn_hex, libpython_hex)
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_happy_path() {
        let dir = tmp();
        let (b, s, l, p) = build_fixture(dir.path());
        let seal = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap();
        assert_eq!(seal.entry_point, ENTRY);
        assert_eq!(seal.bundle_path, dir.path().join("game.bundle"));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_bundle_mismatch() {
        let dir = tmp();
        let (_, s, l, p) = build_fixture(dir.path());
        let bad = "0".repeat(64);
        let err = verify_inner(dir.path(), &bad, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::BundleMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_stdlib_mismatch() {
        let dir = tmp();
        let (b, _, l, p) = build_fixture(dir.path());
        let bad = "f".repeat(64);
        let err = verify_inner(dir.path(), &b, &bad, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::StdlibMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_libdynload_mismatch() {
        let dir = tmp();
        let (b, s, _, p) = build_fixture(dir.path());
        let bad = "a".repeat(64);
        let err = verify_inner(dir.path(), &b, &s, &bad, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibDynloadMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_libpython_mismatch() {
        let dir = tmp();
        let (b, s, l, _) = build_fixture(dir.path());
        let bad = "c".repeat(64);
        let err = verify_inner(dir.path(), &b, &s, &l, &bad, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibpythonMismatch { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_libpython_missing() {
        let dir = tmp();
        let (b, s, l, p) = build_fixture(dir.path());
        fs::remove_file(dir.path().join("python").join("lib").join(SONAME)).unwrap();
        let err = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibpythonNotFound { .. }));
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_site_packages_injection() {
        let dir = tmp();
        let (b, s, l, p) = build_fixture(dir.path());
        // Attacker creates an unshipped site-packages directory that site.py
        // would scan for .pth files at interpreter startup.
        let site_pkg = dir
            .path()
            .join("python")
            .join("lib")
            .join("python3.13")
            .join("site-packages");
        fs::create_dir_all(&site_pkg).unwrap();
        let err = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        match err {
            SealError::UnexpectedPath { path } => assert_eq!(path, site_pkg),
            other => panic!("expected UnexpectedPath, got {other:?}"),
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_detects_dist_packages_injection() {
        // Debian/Ubuntu-flavoured `site.py` also scans `dist-packages`.
        // The seal must catch either name.
        let dir = tmp();
        let (b, s, l, p) = build_fixture(dir.path());
        let dist_pkg = dir
            .path()
            .join("python")
            .join("lib")
            .join("python3.13")
            .join("dist-packages");
        fs::create_dir_all(&dist_pkg).unwrap();
        let err = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        match err {
            SealError::UnexpectedPath { path } => assert_eq!(path, dist_pkg),
            other => panic!("expected UnexpectedPath, got {other:?}"),
        }
    }

    /// Pure layout test — runs on every platform and documents the expected
    /// absent-by-design set for each. Cross-compiles reject cfg-gated tests
    /// are insufficient on their own: Windows CI must exercise the Windows
    /// branch of `absent_by_design_paths`, and this is the cheapest way to
    /// do it without spinning up a full fixture.
    #[test]
    fn absent_by_design_paths_layout() {
        let proj = Path::new("/tmp/fake");
        let dyn_posix = proj
            .join("python")
            .join("lib")
            .join("python3.13")
            .join("lib-dynload");
        let dyn_win = proj.join("python").join("DLLs");

        if cfg!(windows) {
            let got = absent_by_design_paths(proj, &dyn_win);
            assert_eq!(got.len(), 1);
            assert!(got[0].ends_with("python/Lib/site-packages"));
        } else {
            let got = absent_by_design_paths(proj, &dyn_posix);
            assert_eq!(got.len(), 2);
            assert!(got[0].ends_with("python3.13/site-packages"));
            assert!(got[1].ends_with("python3.13/dist-packages"));
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn verify_inner_surfaces_io_error_on_missing_bundle() {
        let dir = tmp();
        // libpython must exist or we'd short-circuit with LibpythonNotFound.
        let libpython = dir.path().join("python").join("lib").join(SONAME);
        write(&libpython, b"fake-libpython-bytes");
        let p = hex_sha256_file(&libpython).unwrap();
        let err = verify_inner(
            dir.path(),
            &"0".repeat(64),
            &"0".repeat(64),
            &"0".repeat(64),
            &p,
            SONAME,
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
        let (b, s, l, p) = build_fixture(dir.path());
        // Flip one byte in a lib-dynload file
        let tampered = dir.path().join("python/lib/python3.13/lib-dynload/_ssl.so");
        write(&tampered, b"TAMPERED");
        let err = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(matches!(err, SealError::LibDynloadMismatch { .. }));
    }

    /// Runtime symlink-injection defense: an attacker with write access to
    /// the dist directory drops `evil.so -> /tmp/payload.so` alongside the
    /// real extensions. The previous behavior (silently skip) left the
    /// lib-dynload hash matching while CPython's import machinery would still
    /// load the shadowed module. `collect_files` must now reject any symlink.
    #[test]
    #[cfg(unix)]
    fn tree_hash_rejects_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let dir = tmp();
        write(&dir.path().join("file.txt"), b"payload");
        let sibling = dir.path().parent().unwrap().join("sibling-target-dir");
        fs::create_dir_all(&sibling).unwrap();
        symlink(&sibling, dir.path().join("symlinked-dir")).unwrap();

        let err = tree_hash(dir.path()).unwrap_err();
        assert!(
            matches!(err, SealError::UnexpectedPath { .. }),
            "expected UnexpectedPath, got {err:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn tree_hash_rejects_file_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tmp();
        write(&dir.path().join("real.txt"), b"real-bytes");
        // `target.txt` lives outside the hashed tree — the concrete
        // injection vector.
        let outside = dir.path().parent().unwrap().join("attacker-payload.txt");
        write(&outside, b"attacker");
        symlink(&outside, dir.path().join("evil.txt")).unwrap();

        let err = tree_hash(dir.path()).unwrap_err();
        match err {
            SealError::UnexpectedPath { path } => {
                assert_eq!(path, dir.path().join("evil.txt"));
            }
            other => panic!("expected UnexpectedPath, got {other:?}"),
        }
    }

    /// End-to-end: build a clean fixture, capture its expected hashes, then
    /// inject a symlink into lib-dynload and confirm `verify_inner` refuses
    /// to boot. Without the `collect_files` symlink guard this test would
    /// pass with the old hash (the attack succeeds silently).
    #[test]
    #[cfg(unix)]
    fn verify_inner_rejects_injected_libdynload_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tmp();
        let (b, s, l, p) = build_fixture(dir.path());

        let dynload = dir
            .path()
            .join("python")
            .join("lib")
            .join("python3.13")
            .join("lib-dynload");
        let payload = dir.path().parent().unwrap().join("evil-payload.so");
        write(&payload, b"attacker-module");
        symlink(&payload, dynload.join("_evil.so")).unwrap();

        let err = verify_inner(dir.path(), &b, &s, &l, &p, SONAME, ZIP_NAME, ENTRY).unwrap_err();
        assert!(
            matches!(err, SealError::UnexpectedPath { .. }),
            "expected UnexpectedPath, got {err:?}"
        );
    }

    #[test]
    fn short_hex_truncates_to_twelve() {
        assert_eq!(short_hex("0123456789abcdef"), "0123456789ab");
        assert_eq!(short_hex("abc"), "abc");
    }
}
