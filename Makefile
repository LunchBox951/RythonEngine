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

.PHONY: run build release dist dist-linux dist-windows dist-macos \
        bootstrap setup-cross test test-rust test-python bundle clean stubs

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

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

define PACKAGE_CMD
	python3 scripts/package.py \
	    --platform      $(PLATFORM) \
	    --arch          $(ARCH) \
	    --game          $(GAME) \
	    --out           dist/$(PLATFORM)-$(ARCH) \
	    --target-python $(VENDOR_PYTHON) \
	    --rust-triple   $(RUST_TRIPLE)
endef

ifeq ($(PLATFORM),macos)
ifneq ($(HOST_PLATFORM),macos)
dist:
	@echo "ERROR: macOS targets require a macOS host (host detected: $(HOST_PLATFORM))." >&2
	@exit 1
else
dist: bootstrap
	PYO3_PYTHON="$(PYO3_PYTHON_BIN)" \
	    cargo build --release --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
endif
else ifeq ($(PLATFORM),windows)
# cargo-xwin lives in ~/.cargo/bin (installed by `make setup-cross`), so no
# venv PATH prepend is needed — only the cross-build env vars matter here.
dist: bootstrap
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	    cargo xwin build --release --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
else
ifeq ($(IS_CROSS),1)
dist: bootstrap
	PATH="$(abspath $(VENV)/bin):$(PATH)" \
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }-L native=$(abspath $(VENDOR_PYTHON)/lib)" \
	    cargo zigbuild --release --target $(RUST_TRIPLE)
	$(PACKAGE_CMD)
else
dist: bootstrap
	PYO3_CROSS=1 \
	PYO3_CROSS_LIB_DIR="$(PYO3_CROSS_LIB_DIR)" \
	PYO3_CROSS_PYTHON_VERSION="$(PYO3_CROSS_PYTHON_VERSION)" \
	RUSTFLAGS="$${RUSTFLAGS:+$$RUSTFLAGS }-L native=$(abspath $(VENDOR_PYTHON)/lib)" \
	    cargo build --release --target $(RUST_TRIPLE)
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
