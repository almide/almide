# The BINARY's version, i.e. Cargo.toml's [package] section — not the first
# `version =` in the file. A [workspace.package] block sits above [package] and
# carries a different number, so `grep ^version | head -1` read 0.12.2 while the
# compiler reported 0.62.0 (#2083). scripts/release-seal.sh hit the same trap and
# was fixed in f84bb6aae; this is its extraction, verbatim.
VERSION := $(shell awk '/^\[/{f=($$0=="[package]")} f && /^version = "/{sub(/^version = "/,""); sub(/".*$$/,""); print; exit}' Cargo.toml)
INSTALL_DIR := $(HOME)/.local/almide
BIN := target/release/almide

.PHONY: build install test test-wasm check clean fmt cross-target verify-trust receipt stdlib-docs

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
	@# Install stdlib sources (read-only, for LSP go-to-definition)
	@if [ -d $(INSTALL_DIR)/stdlib ]; then chmod -R u+w $(INSTALL_DIR)/stdlib; fi
	@rm -rf $(INSTALL_DIR)/stdlib
	@cp -r stdlib $(INSTALL_DIR)/stdlib
	@chmod -R a-w $(INSTALL_DIR)/stdlib
	@echo "Installed almide $(VERSION) to $(INSTALL_DIR)/almide and ~/.local/bin/almide"
	@## Assert rather than print. The line above and the binary disagreed for
	@## fifteen days and nothing noticed, because the mismatch was only ever
	@## shown, never checked. Now a section added above [package] breaks the
	@## build here instead of surfacing in a release artifact.
	@reported=$$($(HOME)/.local/bin/almide --version | awk '{print $$2}'); \
	if [ "$$reported" != "$(VERSION)" ]; then \
		echo "Makefile VERSION is $(VERSION) but the installed binary reports $$reported — see the comment on Makefile:1"; \
		exit 1; \
	fi
	@$(HOME)/.local/bin/almide --version

## Test

test: build
	$(BIN) test

test-rust:
	cargo test --workspace

test-wasm: build
	$(BIN) test --target wasm

test-all: test-rust test test-wasm

cross-target: build
	bash tools/cross-target-check.sh spec/lang
	bash tools/cross-target-check.sh spec/stdlib

## Docs

stdlib-docs:
	@python3 tools/gen-stdlib-doc-index.py

## Check

check:
	cargo check

## Verify the v1 flight-grade trust chain (the third-party "make verify"):
## the Coq proof + independent re-check + axiom audit, then the proof-carrying
## gate (untrusted compiler emits an ownership certificate, the kernel-proven
## checker re-verifies it), then the MIR core + verifier tests. Requires Rocq/Coq.
verify-trust:
	proofs/check.sh
	proofs/gate.sh
	proofs/corpus-wall.sh
	cargo test -p almide-mir
	@## Record WHICH tree+toolchain this verification describes, so a `make
	@## receipt` on the identical tree can fold these verdicts into the receipt
	@## instead of re-deriving them (it runs the same three scripts — 232s of the
	@## CI job). Written only after every step above succeeded; any difference in
	@## the tree or the toolchain yields a different fingerprint and receipt.sh
	@## re-verifies from scratch. See proofs/lib/stamp.sh::toolchain_fingerprint.
	@. proofs/lib/stamp.sh && toolchain_fingerprint . > proofs/.verified-fingerprint

receipt:
	proofs/receipt.sh

# Real verdicts, real trees: the old recipe discarded both exit codes with
# `|| true` and pointed the Almide formatter at src/ — which contains only
# .rs files (#984). The .almd trees CI gates are spec/, examples/ and
# stdlib/ (#919, #1462) — stdlib under --no-import-edit: its sources are
# splice-context, and import auto-insertion is the one transform that
# corrupts them.
fmt:
	cargo fmt
	$(BIN) fmt spec/ examples/
	$(BIN) fmt --no-import-edit stdlib/

## Clean

clean:
	cargo clean
	$(BIN) clean 2>/dev/null || true

## Release
##
## There is no `make release` here. The procedure lives in CLAUDE.md and is
## owned by .github/workflows/release.yml (tag-triggered): an RC channel, the
## release-blocker gate, the interface diff, stamped ledger counts and the
## evidence seal are steps a Makefile target cannot encode, and the two targets
## that used to sit here encoded an older, shorter flow — one that also stamped
## the wrong version into the commit message and the PR title (#2083). A broken
## target is safer than a plausible one that skips the gates.

## Info

version:
	@echo $(VERSION)

help:
	@echo "make build      - Build release binary"
	@echo "make install    - Build + install to ~/.local/almide/"
	@echo "make test       - Run almide spec/ tests"
	@echo "make test-rust  - Run cargo tests"
	@echo "make test-wasm  - Run WASM target tests"
	@echo "make test-all   - Run all test suites"
	@echo "make check      - cargo check"
	@echo "make cross-target - Compare spec test output across native and WASM"
	@echo "make clean      - Clean build artifacts"
	@echo "make version    - Print current version ([package], not [workspace.package])"
