# ---------------------------------------------------------------------------
# project.mk — targets that exist only in USBGuardGUI.
# ---------------------------------------------------------------------------

##@ Packaging

.PHONY: package-deb package-rpm package-arch

package-deb: i18n ## Build the .deb (needs cargo-deb) into dist/
	@command -v cargo-deb >/dev/null || { echo "==> cargo install cargo-deb"; exit 1; }
	@$(CARGO) build --release
	@mkdir -p $(DIST_DIR)
	@$(CARGO) deb --no-build --output $(DIST_DIR)/

package-rpm: i18n ## Build the .rpm (needs cargo-generate-rpm) into dist/
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

##@ Translations

I18N_DOMAIN := usbguard-gui
APP_ID      := io.github.onyks_os.UsbguardGui
LINGUAS     := $(shell grep -v '^\#' po/LINGUAS 2>/dev/null)
ITS_DIR     ?= /usr/share/gettext/its

.PHONY: i18n-pot i18n-update i18n i18n-check

i18n-pot: ## Extract every translatable string into po/usbguard-gui.pot
	@xgettext --language=Rust --from-code=UTF-8 --keyword=gettext --keyword=ngettext:1,2 \
		--add-comments=Translators --sort-by-file --package-name="$(PROJECT_NAME)" \
		--package-version="$(VERSION)" \
		--msgid-bugs-address="https://github.com/$(GITHUB_OWNER)/$(PROJECT_SLUG)/issues" \
		-o po/$(I18N_DOMAIN).pot $(sort $(wildcard src/ui/*.rs))
	@xgettext --join-existing --language=Desktop -o po/$(I18N_DOMAIN).pot data/$(APP_ID).desktop.in
	@xgettext --join-existing --its=$(ITS_DIR)/metainfo.its -o po/$(I18N_DOMAIN).pot data/$(APP_ID).metainfo.xml.in
	@echo "==> po/$(I18N_DOMAIN).pot: $$(grep -c '^msgid "..*"' po/$(I18N_DOMAIN).pot) strings"

i18n-update: i18n-pot ## Merge new and changed strings into every po/<lang>.po
	@for lang in $(LINGUAS); do \
		msgmerge --quiet --update --backup=none po/$$lang.po po/$(I18N_DOMAIN).pot && \
		printf '==> po/%s.po: ' $$lang && msgfmt --statistics -o /dev/null po/$$lang.po; \
	done

i18n: ## Compile the catalogues into target/locale and the translated data files
	@for lang in $(LINGUAS); do \
		mkdir -p target/locale/$$lang/LC_MESSAGES && \
		msgfmt --check -o target/locale/$$lang/LC_MESSAGES/$(I18N_DOMAIN).mo po/$$lang.po || exit 1; \
	done
	@msgfmt --desktop --template data/$(APP_ID).desktop.in -d po -o data/$(APP_ID).desktop
	@msgfmt --xml --template data/$(APP_ID).metainfo.xml.in -d po \
		-o data/$(APP_ID).metainfo.xml

i18n-check: i18n ## Fail when the committed desktop and metainfo files are out of date
	@git diff --exit-code -- data/$(APP_ID).desktop data/$(APP_ID).metainfo.xml \
		|| { echo "==> Run 'make i18n' and commit the result."; exit 1; }
