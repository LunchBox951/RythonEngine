SCRIPT_DIR ?= .
SCRIPT ?= game.scripts.main
OUT ?= bundle.zip

PLATFORM  ?= windows
ARCH      ?= x86_64
GAME      ?= game

VENV ?= .venv

# ── Rust target triple for the requested PLATFORM/ARCH ───────────────────────
ifeq ($(PLATFORM),linux)
  RUST_TRIPLE := $(ARCH)-unknown-linux-gnu
else ifeq ($(PLATFORM),windows)
  # MSVC triple: the vendored python-build-standalone Windows tarball is
  # MSVC-built, so we link against it with cargo-xwin (which bundles the
  # Windows SDK + MSVC CRT). cargo-zigbuild only handles GNU-style linking.
  RUST_TRIPLE := $(ARCH)-pc-windows-msvc
else ifeq ($(PLATFORM),macos)
  RUST_TRIPLE := $(ARCH)-apple-darwin
else
  $(error unknown PLATFORM="$(PLATFORM)" (expected linux|windows|macos))
endif

# ── Host detection (for deciding between PYO3_PYTHON and PYO3_CROSS_*) ───────
# Normalise uname output to match our {linux|windows|macos} / {x86_64|aarch64} taxonomy.
_UNAME_S := $(shell uname -s 2>/dev/null)
_UNAME_M := $(shell uname -m 2>/dev/null)
ifeq ($(_UNAME_S),Linux)
  HOST_PLATFORM := linux
else ifeq ($(_UNAME_S),Darwin)
  HOST_PLATFORM := macos
else ifneq (,$(filter MINGW% MSYS_NT% CYGWIN%,$(_UNAME_S)))
  HOST_PLATFORM := windows
else
  $(error could not determine host platform from uname="$(_UNAME_S)" (expected Linux, Darwin, MINGW*, MSYS_NT*, or CYGWIN*))
endif
ifeq ($(_UNAME_M),arm64)
  HOST_ARCH := aarch64
else
  HOST_ARCH := $(_UNAME_M)
endif

VENDOR_PYTHON := vendor/python/$(PLATFORM)-$(ARCH)/python
BUNDLE_DIR    := target/bundles/$(PLATFORM)-$(ARCH)

# PYO3 env. For native (host == target) non-macOS builds, point PYO3_PYTHON at
# the vendored interpreter. For cross (host != target) builds, use the
# PYO3_CROSS_LIB_DIR / PYO3_CROSS_PYTHON_VERSION pair — the target interpreter
# can't be executed on the host. Windows link stubs live in python/libs/;
# Linux/macOS libpython lives in python/lib/.
ifeq ($(PLATFORM),windows)
  PYO3_CROSS_LIB_DIR := $(abspath $(VENDOR_PYTHON)/libs)
else
  PYO3_CROSS_LIB_DIR := $(abspath $(VENDOR_PYTHON)/lib)
endif
PYO3_CROSS_PYTHON_VERSION := $(shell python3 -c "import json; print(json.load(open('scripts/python_standalone_pins.json'))['python_version'])")
ifeq ($(PLATFORM),windows)
  PYO3_PYTHON_BIN := $(abspath $(VENDOR_PYTHON)/python.exe)
else
  PYO3_PYTHON_BIN := $(abspath $(VENDOR_PYTHON)/bin/python3)
endif

# True (= 1) when the Rust target triple differs from the host — i.e., PyO3
# cross-compile mode should kick in.
ifeq ($(HOST_PLATFORM)-$(HOST_ARCH),$(PLATFORM)-$(ARCH))
  IS_CROSS := 0
else
  IS_CROSS := 1
endif

.PHONY: run build release seal-bundle dist dist-linux dist-windows dist-macos \
        bootstrap setup-cross test test-rust test-python bundle clean stubs

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

# Unsealed release binary — refuses release mode at runtime. Useful for local
# smoke-builds that skip the full stdlib compile step. Use `make dist` for a
# shippable sealed distribution.
release:
	cargo build --release

# ── Cross-build toolchain setup ──────────────────────────────────────────────
#
# cargo-zigbuild and cargo-xwin are Rust binaries — they must be installed via
# `cargo install`, not pip (cargo-xwin has no PyPI package). ziglang IS a PyPI
# package: it ships the `zig` compiler shim that cargo-zigbuild invokes; we
# keep it in a project-local venv so the host system Python isn't touched.

$(VENV)/bin/pip:
	python3 -m venv $(VENV)

setup-cross: $(VENV)/bin/pip
	$(VENV)/bin/pip install --upgrade ziglang
	cargo install --locked cargo-zigbuild cargo-xwin
	rustup target add \
	    x86_64-unknown-linux-gnu \
	    aarch64-unknown-linux-gnu \
	    x86_64-pc-windows-msvc \
	    aarch64-pc-windows-msvc

# ── Vendored target Python ───────────────────────────────────────────────────

bootstrap:
	python3 scripts/bootstrap_target.py $(PLATFORM) $(ARCH)

# ── Sealed bundle generation ────────────────────────────────────────────────
#
# bundle.py reads stdlib/lib-dynload/libpython from the vendored target tree,
# pre-compiles them, and emits hashes.env. The env file is consumed verbatim
# by the subsequent cargo build so RYTHON_*_HASH constants are baked into the
# release binary — release_seal.rs at runtime refuses to boot on any mismatch.

seal-bundle: bootstrap
	python3 scripts/bundle.py \
	    --platform      $(PLATFORM) \
	    --target-python $(VENDOR_PYTHON) \
	    --game          $(GAME) \
	    --out-dir       $(BUNDLE_DIR)

# ── Distribution packaging ───────────────────────────────────────────────────
#
# Linux targets: cargo-zigbuild for cross (different arch), plain cargo for
# native (avoids zigbuild's older-glibc mismatch against host /usr/lib libs).
# Windows targets: cargo-xwin (bundles MSVC SDK + CRT; matches python-build-
# standalone's MSVC ABI). macOS target: plain cargo; install_name_tool is
# Darwin-only, so this branch hard-errors on non-macOS hosts.
#
# PYO3_CROSS=1 is set for Linux and Windows to force PyO3 cross mode. Without
# it, PyO3 runs the vendored interpreter's sysconfig which reports a bogus
# LIBDIR=/install/lib (a python-build-standalone artifact), breaking link
# path discovery.
#
# hashes.env is loaded *and* asserted non-empty with RYTHON_SEALED=1 before
# cargo runs — a half-successful seal-bundle or a missing file would
# otherwise silently produce an unsealed binary the dev thinks is sealed.
# --locked guarantees the sealed build does not silently update the
# dependency graph between seal-bundle and cargo invocations.

define ASSERT_HASHES_ENV
	@set -eu; \
	 hashes_env="$(BUNDLE_DIR)/hashes.env"; \
	 if [ ! -s "$$hashes_env" ]; then \
	   echo "ERROR: $$hashes_env missing or empty — seal-bundle did not complete" >&2; \
	   exit 1; \
	 fi; \
	 if ! grep -qx 'RYTHON_SEALED=1' "$$hashes_env"; then \
	   echo "ERROR: $$hashes_env is missing RYTHON_SEALED=1 — refusing to build" >&2; \
	   exit 1; \
	 fi
endef

define PACKAGE_CMD
	python3 scripts/package.py \
	    --platform      $(PLATFORM) \
	    --arch          $(ARCH) \
	    --game          $(GAME) \
	    --out           dist/$(PLATFORM)-$(ARCH) \
	    --target-python $(VENDOR_PYTHON) \
	    --bundle-dir    $(BUNDLE_DIR) \
	    --rust-triple   $(RUST_TRIPLE)
endef

ifeq ($(PLATFORM),macos)
ifneq ($(HOST_PLATFORM),macos)
dist:
	@echo "ERROR: macOS targets require a macOS host (host detected: $(HOST_PLATFORM))." >&2
	@exit 1
else
dist: seal-bundle
	$(ASSERT_HASHES_ENV)
	env $$(grep -v '^$$' $(BUNDLE_DIR)/hashes.env | xargs) \
	PYO3_PYTHON="$(PYO3_PYTHON_BIN)" \
	    cargo build --release --locked --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
endif
else ifeq ($(PLATFORM),windows)
# cargo-xwin lives in ~/.cargo/bin (installed by `make setup-cross`), so no
# venv PATH prepend is needed — only the cross-build env vars matter here.
dist: seal-bundle
	$(ASSERT_HASHES_ENV)
	env $$(grep -v '^$$' $(BUNDLE_DIR)/hashes.env | xargs) \
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	    cargo xwin build --release --locked --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
else
ifeq ($(IS_CROSS),1)
dist: seal-bundle
	$(ASSERT_HASHES_ENV)
	env $$(grep -v '^$$' $(BUNDLE_DIR)/hashes.env | xargs) \
	PATH="$(abspath $(VENV)/bin):$$PATH" \
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }-L native=$(abspath $(VENDOR_PYTHON)/lib)" \
	    cargo zigbuild --release --locked --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
else
dist: seal-bundle
	$(ASSERT_HASHES_ENV)
	env $$(grep -v '^$$' $(BUNDLE_DIR)/hashes.env | xargs) \
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }-L native=$(abspath $(VENDOR_PYTHON)/lib)" \
	    cargo build --release --locked --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
endif
endif

dist-linux:
	$(MAKE) dist PLATFORM=linux ARCH=$(ARCH) GAME=$(GAME)

dist-windows:
	$(MAKE) dist PLATFORM=windows ARCH=$(ARCH) GAME=$(GAME)

dist-macos:
	$(MAKE) dist PLATFORM=macos ARCH=$(ARCH) GAME=$(GAME)

test: test-rust test-python

test-rust:
	cargo test --workspace

test-python: build
	python3 tests/python/run_tests.py

bundle:
	@echo "Bundling scripts from $(SCRIPT_DIR) into $(OUT)..."
	cd $(SCRIPT_DIR) && zip -r ../$(OUT) .

clean:
	cargo clean

stubs:
	python3 -m venv $(VENV) || true
	$(VENV)/bin/pip install -e .
