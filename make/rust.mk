# ---------------------------------------------------------------------------
# rust.mk — Rust implementation of the lang-* target contract.
# ---------------------------------------------------------------------------

CARGO ?= cargo

.PHONY: lang-setup lang-lint lang-format lang-test lang-test-integration lang-fuzz lang-audit lang-build lang-clean coverage doc

lang-setup:
	@echo "==> [$(PROJECT_SHORT)] Installing toolchain components..."
	@rustup component add clippy rustfmt 2>/dev/null || true
	@$(CARGO) fetch

lang-lint:
	@echo "==> [$(PROJECT_SHORT)] rustfmt check..."
	@$(CARGO) fmt --all -- --check
	@echo "==> [$(PROJECT_SHORT)] Clippy (warnings are errors)..."
	@$(CARGO) clippy --all-targets --all-features -- -D warnings

lang-format:
	@echo "==> [$(PROJECT_SHORT)] Auto-formatting..."
	@$(CARGO) fmt --all
	@$(CARGO) clippy --all-targets --all-features --fix --allow-dirty --allow-staged || true

lang-test:
	@echo "==> [$(PROJECT_SHORT)] Unit tests..."
	@$(CARGO) test --all-features

lang-test-integration:
	@echo "==> [$(PROJECT_SHORT)] Integration tests..."
	@$(CARGO) test --all-features --test '*' -- --ignored

lang-fuzz:
	@echo "==> [$(PROJECT_SHORT)] Property-based tests (proptest)..."
	@PROPTEST_CASES=$${PROPTEST_CASES:-1024} $(CARGO) test --all-features properties

lang-audit:
	@echo "==> [$(PROJECT_SHORT)] cargo-audit..."
	@if command -v cargo-audit >/dev/null 2>&1; then \
		$(CARGO) audit; \
	else \
		echo "==> cargo-audit not found. Install: cargo install cargo-audit"; exit 1; \
	fi

lang-build:
	@echo "==> [$(PROJECT_SHORT)] Building release binary..."
	@$(CARGO) build --release
	@mkdir -p $(DIST_DIR)
	@cp target/release/$(PROJECT_DIST) $(DIST_DIR)/ 2>/dev/null || true
	@$(CARGO) package --allow-dirty --target-dir target/package >/dev/null 2>&1 || true
	@cp target/package/package/*.crate $(DIST_DIR)/ 2>/dev/null || true

lang-clean:
	@$(CARGO) clean

##@ Rust extras

coverage: coverage-app ## Enforce whole-application coverage and produce an HTML report
	@$(CARGO) llvm-cov report --html

doc: ## Build and open the rustdoc API documentation
	@$(CARGO) doc --no-deps --all-features --open

.PHONY: coverage-core
coverage-core: ## Measure headless coverage and require >90% per rule/operation-core file
	@$(CARGO) llvm-cov --locked --no-default-features --json --output-path target/coverage-headless.json
	@python3 scripts/check-coverage.py --core target/coverage-headless.json

.PHONY: reproducible test-reproducible
REPRO_ARGS ?=
reproducible: ## Compare two clean offline GUI binaries, RPMs, DEBs, Arch packages and Flatpak bundles in the pinned container (REPRO_ARGS=--worktree for local changes)
	@python3 scripts/reproducible-build.py $(REPRO_ARGS)

test-reproducible: ## Test source normalization and the reproducibility checker's failure paths
	@python3 -m unittest discover -s tests -p 'test_reproducible*.py' -v

.PHONY: coverage-app test-coverage
test-coverage: ## Prove coverage thresholds reject missing files and rounded percentages
	@python3 -m unittest discover -s tests -p 'test_coverage.py' -v

coverage-app: test-coverage ## Require >=98% production line coverage, including GTK and both feature configurations
	@$(CARGO) llvm-cov clean --workspace
	@$(CARGO) llvm-cov --locked --all-features --no-report
	@$(CARGO) llvm-cov --locked --no-default-features --no-report --test cli
	@scripts/test-gui.sh $(CARGO) llvm-cov --locked --all-features --no-report --lib -- gui_application_contracts --ignored --test-threads=1
	@scripts/test-gui.sh $(CARGO) llvm-cov --locked --all-features --no-report --lib -- gui_device_actions --ignored --test-threads=1
	@scripts/test-gui.sh $(CARGO) llvm-cov --locked --all-features --no-report --test gui_lifecycle -- --ignored --test-threads=1
	@scripts/test-gui.sh $(CARGO) llvm-cov --locked --all-features --no-report --lib -- gui_authentication_notices --ignored --test-threads=1
	@$(CARGO) llvm-cov report --json --output-path target/coverage-application.json
	@python3 scripts/check-coverage.py target/coverage-application.json
	@python3 scripts/check-coverage.py --core target/coverage-application.json
