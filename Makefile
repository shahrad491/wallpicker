# User installation is explicit: make install. Packaging stays inside target/.
PREFIX ?= $(HOME)/.local
DESTDIR ?=
CARGO ?= cargo

.PHONY: all build check test run package package-deb package-appimage package-check install help
all: build

build:
	$(CARGO) build --release --locked

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --locked --all-targets -- -D warnings
	$(CARGO) test --locked

test:
	$(CARGO) test --locked

run:
	$(CARGO) run --release --locked

package:
	CARGO="$(CARGO)" ./scripts/package.sh

package-deb:
	CARGO="$(CARGO)" ./scripts/package.sh --format deb

package-appimage:
	CARGO="$(CARGO)" ./scripts/package.sh --format appimage

package-check:
	./scripts/test-package.sh

install: build
	./scripts/install.sh --binary "$${CARGO_TARGET_DIR:-target}/release/wallpicker" --prefix "$(PREFIX)" --destdir "$(DESTDIR)"

help:
	@printf '%s\n' 'make build          Build optimized binary with Cargo.lock' 'make run            Open the optimized picker' 'make check          Format, Clippy, and tests' 'make package        Build archive and checksum under target/dist/' 'make package-deb    Build a host-native .deb (see compatibility notes)' 'make package-appimage  Build if linuxdeploy/appimagetool are available' 'make package-check  Smoke-test packaging and staged installation' 'make install        Build and install to PREFIX (default ~/.local)' 'make install PREFIX=/usr DESTDIR="$$PWD/target/stage"  Stage a system layout'
