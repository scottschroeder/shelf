SHELL := /bin/bash

CARGO = cargo
CARGO_OPTS =
FMT_OPTS = 

CURRENT_DIR = $(shell pwd)
VERSION=$(shell grep -Em1 "^version" Cargo.toml | sed -r 's/.*"(.*)".*/\1/')
NAME := shelf

.PHONY: all fmt-check test test_unit test_local test_arch test_ubuntu test_platforms lint

all: pre-commit
pre-commit: fix fmt test

build_debug:
	$(CARGO) $(CARGO_OPTS) build

clean:
	$(CARGO) $(CARGO_OPTS) clean

fmt: CARGO_OPTS += +nightly
fmt:
	$(CARGO) $(CARGO_OPTS) fmt --all -- $(FMT_OPTS)

fmt-check: FMT_OPTS += --check
fmt-check: fmt

fix:
	$(CARGO) $(CARGO_OPTS) fix --allow-staged
	$(CARGO) $(CARGO_OPTS) clippy --fix --allow-staged --allow-dirty

lint:
	$(CARGO) $(CARGO_OPTS) clippy --workspace --all-targets -- -D clippy::correctness

test:
	$(CARGO) $(CARGO_OPTS) test
