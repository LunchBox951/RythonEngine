SCRIPT_DIR ?= scripts
SCRIPT ?=
OUT ?= bundle.zip

.PHONY: run build release test bundle clean stubs

run:
	cargo run -p rython-cli -- --script-dir $(SCRIPT_DIR) $(if $(SCRIPT),--entry-point $(SCRIPT))

build:
	cargo build

release:
	cargo build --release

test:
	cargo test --workspace

bundle:
	@echo "Bundling scripts from $(SCRIPT_DIR) into $(OUT)..."
	cd $(SCRIPT_DIR) && zip -r ../$(OUT) .

clean:
	cargo clean

stubs:
	python3 -m venv .venv || true
	.venv/bin/pip install -e .
