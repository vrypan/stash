CARGO   := cargo
PREFIX  ?= /usr/local

# Feature flag that enables stash-completion binary
COMPLETION_FEATURE := --features completion

.PHONY: build build-all release release-all check test bench clean install install-all help

## build       Build stash (dev, no completion)
build:
	$(CARGO) build

## build-all   Build stash + stash-completion (dev)
build-all:
	$(CARGO) build $(COMPLETION_FEATURE)

## release     Build stash (release, no completion)
release:
	$(CARGO) build --release

## release-all Build stash + stash-completion (release)
release-all:
	$(CARGO) build --release $(COMPLETION_FEATURE)

## check       Run cargo check (fast syntax/type check)
check:
	$(CARGO) check $(COMPLETION_FEATURE)

## test        Run all tests
test:
	$(CARGO) test

## bench       Run benchmarks
bench:
	$(CARGO) bench

## clean       Remove build artifacts
clean:
	$(CARGO) clean

## install     Install stash to PREFIX/bin (default: /usr/local/bin)
install: release
	install -d "$(PREFIX)/bin"
	install -m 755 target/release/stash "$(PREFIX)/bin/stash"

## install-all Install stash + stash-completion to PREFIX/bin
install-all: release-all
	install -d "$(PREFIX)/bin"
	install -m 755 target/release/stash "$(PREFIX)/bin/stash"
	install -m 755 target/release/stash-completion "$(PREFIX)/bin/stash-completion"

## help        Show this help
help:
	@grep -E '^##' Makefile | sed 's/^## //'
