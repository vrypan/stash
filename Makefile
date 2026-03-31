BIN      := stash
VERSION  := $(shell git describe --tags --exact-match 2>/dev/null || git rev-parse --short HEAD 2>/dev/null || echo dev)
LDFLAGS  := -s -w -X stash/cmd.Version=$(VERSION)
GCFLAGS  := all=-trimpath=$(CURDIR)
ASMFLAGS := all=-trimpath=$(CURDIR)

.PHONY: build clean

build:
	go build -ldflags="$(LDFLAGS)" -gcflags="$(GCFLAGS)" -asmflags="$(ASMFLAGS)" -o $(BIN) .

clean:
	rm -f $(BIN)
