.PHONY: all build serve

PORT ?= 8877

all: build serve

build:
	bash scripts/build-web.sh

serve:
	node scripts/serve-web.mjs $(PORT)

.PHONY: fmt lint hooks

fmt:
	cargo fmt
	cargo fmt --manifest-path xtask/Cargo.toml

lint:
	cargo xtask

hooks:
	@current=$$(git config --get core.hooksPath || true); \
	if [ -n "$$current" ] && [ "$$current" != .githooks ]; then \
		echo "Existing core.hooksPath=$$current; integrate .githooks/pre-commit there first."; exit 1; \
	fi
	git config core.hooksPath .githooks

.PHONY: test-lint
test-lint:
	cargo test --manifest-path xtask/Cargo.toml
