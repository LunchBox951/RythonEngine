SCRIPT_DIR ?= .
SCRIPT ?= game.scripts.main
OUT ?= bundle.zip

PLATFORM   ?= windows
ARCH       ?= x86_64
GAME       ?= game
BUNDLE_DIR ?= target/bundles

.PHONY: run build release seal-bundle dist dist-linux dist-windows dist-macos test test-rust test-python bundle clean stubs

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

# Unsealed release binary — refuses release mode at runtime. Useful for local
# smoke-builds that skip the full stdlib compile step. Use `make dist` for a
# shippable sealed distribution.
release:
	cargo build --release

# Pre-build game + stdlib .pyc, copy lib-dynload, emit hashes.env. Must run
# before the sealed cargo build so RYTHON_*_HASH can be baked into the binary.
seal-bundle:
	python3 scripts/bundle.py --game $(GAME) --out-dir $(BUNDLE_DIR)

# Full sealed distribution: bundle → hash → compile-with-env → package.
# `env $(cat hashes.env)` injects RYTHON_*_HASH into cargo's env so that
# build.rs forwards them to rustc as compile-time constants.
dist: seal-bundle
	env $$(cat $(BUNDLE_DIR)/hashes.env | grep -v '^$$' | xargs) cargo build --release --locked
	python3 scripts/package.py \
	    --platform   $(PLATFORM) \
	    --arch       $(ARCH) \
	    --game       $(GAME) \
	    --out        dist/$(PLATFORM)-$(ARCH) \
	    --bundle-dir $(BUNDLE_DIR)

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
	python3 -m venv .venv || true
	.venv/bin/pip install -e .
