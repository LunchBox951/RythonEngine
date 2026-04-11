SCRIPT_DIR ?= scripts
SCRIPT ?=
OUT ?= bundle.zip

PLATFORM  ?= windows
ARCH      ?= x86_64
GAME      ?= game

.PHONY: run build release dist dist-linux dist-windows dist-macos test test-rust test-python bundle clean stubs

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

release:
	cargo build --release

dist: release
	python3 scripts/package.py \
	    --platform $(PLATFORM) \
	    --arch     $(ARCH) \
	    --game     $(GAME) \
	    --out      dist/$(PLATFORM)-$(ARCH)

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
