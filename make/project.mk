# ---------------------------------------------------------------------------
# project.mk — targets that exist only in USBGuardGUI.
# ---------------------------------------------------------------------------

##@ Packaging

.PHONY: package-deb package-rpm package-arch

package-deb: ## Build the .deb (needs cargo-deb) into dist/
	@command -v cargo-deb >/dev/null || { echo "==> cargo install cargo-deb"; exit 1; }
	@$(CARGO) build --release
	@mkdir -p $(DIST_DIR)
	@$(CARGO) deb --no-build --output $(DIST_DIR)/

package-rpm: ## Build the .rpm (needs cargo-generate-rpm) into dist/
	@command -v cargo-generate-rpm >/dev/null || { echo "==> cargo install cargo-generate-rpm"; exit 1; }
	@$(CARGO) build --release
	@mkdir -p $(DIST_DIR)
	@$(CARGO) generate-rpm --output $(DIST_DIR)/

package-arch: ## Build the Arch package from the current tree (on Arch; needs makepkg)
	@command -v makepkg >/dev/null || { echo "==> makepkg is only available on Arch"; exit 1; }
	@cd packaging/arch && makepkg -f

##@ Fuzzing

FUZZ_RUNS ?= 1000000

.PHONY: fuzz-parser

fuzz-parser: ## cargo-fuzz the rule parser (nightly + cargo-fuzz); FUZZ_RUNS=… to change the count
	@command -v cargo-fuzz >/dev/null || { echo "==> cargo install cargo-fuzz"; exit 1; }
	@# The starting corpus is the synthetic fixture set, one rule per file.
	@for target in parse_rule round_trip; do \
		mkdir -p fuzz/corpus/$$target; \
		grep -hv -e '^#' -e '^$$' tests/fixtures/rules/*.rules | while IFS= read -r line; do \
			printf '%s' "$$line" > "fuzz/corpus/$$target/$$(printf '%s' "$$line" | sha1sum | cut -c1-16)"; \
		done; \
		(cd fuzz && cargo +nightly fuzz run $$target corpus/$$target -- -runs=$(FUZZ_RUNS) -max_len=4096) || exit 1; \
	done
