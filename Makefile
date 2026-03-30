VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
INSTALL_DIR := $(HOME)/.local/almide
BIN := target/release/almide

.PHONY: build install test test-wasm test-ts check clean fmt release parity cross-target cheatsheet stdlib-docs

## Build

build:
	cargo build --release

## Install

install: build
	@mkdir -p $(INSTALL_DIR)
	cp $(BIN) $(INSTALL_DIR)/almide
	@mkdir -p $(HOME)/.local/bin
	rm -f $(HOME)/.local/bin/almide
	cp $(BIN) $(HOME)/.local/bin/almide
	@echo "Installed almide $(VERSION) to $(INSTALL_DIR)/almide and ~/.local/bin/almide"
	@almide --version

## Test

test: build
	$(BIN) test

test-rust:
	cargo test

test-wasm: build
	$(BIN) test --target wasm

test-ts: build
	$(BIN) test --target ts

test-all: test-rust test test-wasm test-ts parity

## Parity

parity:
	bash tools/stdlib-parity-check.sh

cross-target: build
	bash tools/cross-target-check.sh spec/lang
	bash tools/cross-target-check.sh spec/stdlib

## Docs

cheatsheet:
	@bash tools/generate-stdlib-cheatsheet.sh > /tmp/stdlib-section.md
	@echo "Generated stdlib section. Review and update docs/CHEATSHEET.md manually, or run:"
	@echo "  make cheatsheet-update"

cheatsheet-update:
	@python3 tools/update-cheatsheet-stdlib.py
	@echo "Updated docs/CHEATSHEET.md stdlib section from TOML definitions."

stdlib-docs:
	@python3 tools/generate-stdlib-docs.py --write
	@echo "Regenerated docs-site/src/content/docs/stdlib/ from TOML definitions."

## Check

check:
	cargo check

fmt:
	cargo fmt --check 2>/dev/null || true
	$(BIN) fmt src/ 2>/dev/null || true

## Clean

clean:
	cargo clean
	$(BIN) clean 2>/dev/null || true

## Release (bump version, build, install, commit, push, create PR)

release: test-all
	@echo "All tests passed. Creating release v$(VERSION)..."
	git add Cargo.toml Cargo.lock README.md
	git commit -m "Bump version to $(VERSION)"
	git push origin develop
	@echo "Pushed. Create PR with: make pr"

pr:
	@MAIN_SHA=$$(git rev-parse origin/main) && \
	BODY=$$(git log --oneline $$MAIN_SHA..HEAD | sed 's/^/- /') && \
	gh pr create \
		--base main \
		--head develop \
		--title "v$(VERSION)" \
		--body "$$BODY"

## Info

version:
	@echo $(VERSION)

help:
	@echo "make build      - Build release binary"
	@echo "make install    - Build + install to ~/.local/almide/"
	@echo "make test       - Run almide spec/exercise tests"
	@echo "make test-rust  - Run cargo tests"
	@echo "make test-wasm  - Run WASM target tests"
	@echo "make test-ts    - Run TS target tests"
	@echo "make test-all   - Run all test suites"
	@echo "make check      - cargo check"
	@echo "make parity     - Verify stdlib parity across RS/TS/WASM"
	@echo "make cross-target - Run spec tests on both Rust and TS targets"
	@echo "make clean      - Clean build artifacts"
	@echo "make release    - Test + commit + push version bump"
	@echo "make pr         - Create PR from develop to main"
	@echo "make version    - Print current version"
