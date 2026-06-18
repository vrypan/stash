ZIG     := zig
PREFIX  ?= /usr/local

.PHONY: build release check test clean install size help

## build       Build stash (debug)
build:
	$(ZIG) build

## release     Build stash optimized for size
release:
	$(ZIG) build -Doptimize=ReleaseSmall

## check       Type-check by building stash
check:
	$(ZIG) build

## test        Run unit tests
test:
	$(ZIG) build test

## clean       Remove Zig build artifacts
clean:
	rm -rf zig-out .zig-cache zig-pkg

## install     Install stash to PREFIX/bin (default: /usr/local/bin)
install: release
	install -d "$(PREFIX)/bin"
	install -m 755 zig-out/bin/stash "$(PREFIX)/bin/stash"
	install -m 755 zig-out/bin/stash-bookmark "$(PREFIX)/bin/stash-bookmark"

## size        Show Zig stash binary size
size: release
	ls -lh zig-out/bin/stash

## help        Show this help
help:
	@grep -E '^##' Makefile | sed 's/^## //'
