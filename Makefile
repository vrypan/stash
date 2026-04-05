BIN := stash
CARGO := cargo
PREFIX ?= /usr/local

.PHONY: build release run check test bench clean install

build:
	$(CARGO) build

release:
	$(CARGO) build --release

run:
	$(CARGO) run -- $(ARGS)

check:
	$(CARGO) check

test:
	$(CARGO) test

bench:
	$(CARGO) bench

clean:
	$(CARGO) clean

install: release
	install -d "$(PREFIX)/bin"
	install -m 755 target/release/$(BIN) "$(PREFIX)/bin/$(BIN)"
