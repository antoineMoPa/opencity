.PHONY: all build serve

PORT ?= 8877

all: build serve

build:
	bash scripts/build-web.sh

serve:
	python3 -m http.server --bind 127.0.0.1 --directory web/dist $(PORT)
