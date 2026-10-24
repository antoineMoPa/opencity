.PHONY: all build serve

PORT ?= 8877

all: build serve

build:
	bash scripts/build-web.sh

serve:
	node scripts/serve-web.mjs $(PORT)
