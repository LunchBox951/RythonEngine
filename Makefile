# ── Defaults ──────────────────────────────────────────────────────
SCRIPT_DIR  ?= game/scripts
GAME_DIR    ?= game
SCRIPT      ?=
OUT         ?= bundle.zip
DIST_DIR    ?= dist
PYTHON_VER  ?= 3.12.9
VERSION     := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

# ── Platform toggle flags (set to 1 to enable) ───────────────────
WINDOWS     ?=
LINUX       ?=
MACOS       ?=

# ── Rust cross-compilation targets ───────────────────────────────
TARGET_WINDOWS := x86_64-pc-windows-gnu
TARGET_LINUX   := x86_64-unknown-linux-gnu
TARGET_MACOS   := x86_64-apple-darwin

# ── Host detection ───────────────────────────────────────────────
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Linux)
  HOST_PLATFORM := linux
endif
ifeq ($(UNAME_S),Darwin)
  HOST_PLATFORM := macos
endif

# ── python-build-standalone release tag & base URL ───────────────
PBS_TAG     := 20240909
PBS_BASE    := https://github.com/indygreg/python-build-standalone/releases/download/$(PBS_TAG)

# ── Phony targets ────────────────────────────────────────────────
.PHONY: run run-release build release test bundle clean stubs \
        editor check fmt docs info dist \
        _release-build _dist-package _fetch-python

# ══════════════════════════════════════════════════════════════════
#  Development
# ══════════════════════════════════════════════════════════════════

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

run-release:
	cargo run --release -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

editor:
	cargo run -p rython-editor

stubs:
	python3 -m venv .venv || true
	.venv/bin/pip install -e .

# ══════════════════════════════════════════════════════════════════
#  Quality
# ══════════════════════════════════════════════════════════════════

test:
	cargo test --workspace

check:
	cargo fmt --check
	cargo clippy --workspace -- -D warnings
	cargo test --workspace

fmt:
	cargo fmt

docs:
	cargo doc --workspace --no-deps --open

# ══════════════════════════════════════════════════════════════════
#  Release builds
# ══════════════════════════════════════════════════════════════════

# Build release binaries for selected platforms.
#   make release                          # host platform only
#   make release LINUX=1                  # linux only
#   make release WINDOWS=1 LINUX=1        # windows + linux
#   make release WINDOWS=1 LINUX=1 MACOS=1
release:
	@if [ -z "$(WINDOWS)$(LINUX)$(MACOS)" ]; then \
		echo "==> Building release for host platform ($(HOST_PLATFORM))..."; \
		$(MAKE) _release-build TARGET=$(TARGET_$(shell echo $(HOST_PLATFORM) | tr a-z A-Z)); \
	else \
		$(if $(LINUX),echo "==> Building for Linux..."   && $(MAKE) _release-build TARGET=$(TARGET_LINUX) &&) \
		$(if $(WINDOWS),echo "==> Building for Windows..." && $(MAKE) _release-build TARGET=$(TARGET_WINDOWS) &&) \
		$(if $(MACOS),echo "==> Building for macOS..."   && $(MAKE) _release-build TARGET=$(TARGET_MACOS) &&) \
		echo "==> Release builds complete."; \
	fi

_release-build:
	@if [ "$(TARGET)" = "$(TARGET_LINUX)" ] && [ "$(HOST_PLATFORM)" = "linux" ] || \
	    [ "$(TARGET)" = "$(TARGET_MACOS)" ] && [ "$(HOST_PLATFORM)" = "macos" ]; then \
		cargo build --release -p rython-cli --target $(TARGET); \
	else \
		command -v cross >/dev/null 2>&1 || { echo "Error: 'cross' not found. Install with: cargo install cross"; exit 1; }; \
		cross build --release -p rython-cli --target $(TARGET); \
	fi

# ══════════════════════════════════════════════════════════════════
#  Bundle & distribute
# ══════════════════════════════════════════════════════════════════

bundle:
	@echo "Bundling scripts from $(SCRIPT_DIR) into $(OUT)..."
	@cd $(SCRIPT_DIR) && zip -r $(CURDIR)/$(OUT) .

# Create complete distributable packages.
#   make dist                             # host platform only
#   make dist LINUX=1 WINDOWS=1 MACOS=1   # all three
dist: release bundle
	@if [ -z "$(WINDOWS)$(LINUX)$(MACOS)" ]; then \
		$(MAKE) _dist-package PLATFORM=$(HOST_PLATFORM) \
			TARGET=$(TARGET_$(shell echo $(HOST_PLATFORM) | tr a-z A-Z)); \
	else \
		$(if $(LINUX),$(MAKE) _dist-package PLATFORM=linux TARGET=$(TARGET_LINUX) &&) \
		$(if $(WINDOWS),$(MAKE) _dist-package PLATFORM=windows TARGET=$(TARGET_WINDOWS) &&) \
		$(if $(MACOS),$(MAKE) _dist-package PLATFORM=macos TARGET=$(TARGET_MACOS) &&) \
		echo "==> All packages complete."; \
	fi

_dist-package:
	@PKG=$(DIST_DIR)/rython-$(VERSION)-$(PLATFORM) && \
	rm -rf $$PKG && mkdir -p $$PKG && \
	echo "==> Packaging for $(PLATFORM)..." && \
	if [ "$(PLATFORM)" = "windows" ]; then \
		cp target/$(TARGET)/release/rython.exe $$PKG/; \
	else \
		cp target/$(TARGET)/release/rython $$PKG/; \
	fi && \
	cp $(OUT) $$PKG/ && \
	mkdir -p $$PKG/game && \
	cp $(GAME_DIR)/project.json $$PKG/game/ && \
	cp -r $(GAME_DIR)/assets  $$PKG/game/ && \
	cp -r $(GAME_DIR)/scenes  $$PKG/game/ && \
	cp -r $(GAME_DIR)/ui      $$PKG/game/ && \
	$(MAKE) _fetch-python PLATFORM=$(PLATFORM) DEST=$$PKG/python && \
	echo "==> Creating archive..." && \
	cd $(DIST_DIR) && tar czf rython-$(VERSION)-$(PLATFORM).tar.gz $$(basename $$PKG) && \
	echo "==> Done: $(DIST_DIR)/rython-$(VERSION)-$(PLATFORM).tar.gz"

_fetch-python:
	@mkdir -p $(DEST) && \
	case "$(PLATFORM)" in \
		linux)   PBS_FILE="cpython-$(PYTHON_VER)+$(PBS_TAG)-x86_64-unknown-linux-gnu-install_only_stripped.tar.gz" ;; \
		macos)   PBS_FILE="cpython-$(PYTHON_VER)+$(PBS_TAG)-x86_64-apple-darwin-install_only_stripped.tar.gz" ;; \
		windows) PBS_FILE="cpython-$(PYTHON_VER)+$(PBS_TAG)-x86_64-pc-windows-msvc-shared-install_only_stripped.tar.gz" ;; \
		*)       echo "Error: unknown platform '$(PLATFORM)'"; exit 1 ;; \
	esac && \
	CACHE_DIR=.cache/python && mkdir -p $$CACHE_DIR && \
	if [ ! -f "$$CACHE_DIR/$$PBS_FILE" ]; then \
		echo "Downloading Python $(PYTHON_VER) for $(PLATFORM)..." && \
		curl -fSL -o "$$CACHE_DIR/$$PBS_FILE" "$(PBS_BASE)/$$PBS_FILE"; \
	else \
		echo "Using cached Python for $(PLATFORM)"; \
	fi && \
	tar xzf "$$CACHE_DIR/$$PBS_FILE" -C $(DEST) --strip-components=1

# ══════════════════════════════════════════════════════════════════
#  Housekeeping
# ══════════════════════════════════════════════════════════════════

clean:
	cargo clean
	rm -rf $(DIST_DIR) $(OUT)

info:
	@echo "RythonEngine v$(VERSION)"
	@echo "Host: $(HOST_PLATFORM)"
	@echo "Rust: $$(rustc --version)"
	@echo "Python: $$(python3 --version 2>/dev/null || echo 'not found')"
