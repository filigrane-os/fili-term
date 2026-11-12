# Aéroport (fili-term): build and install.
#
#   make                       build the release binaries (window and session daemon)
#   make test                  unit tests
#   sudo make install          install under /usr/local (PREFIX=/usr for a system package)
#   make install DESTDIR=pkg PREFIX=/usr      stage into a directory (packagers)
#
# cxx-qt finds Qt through qmake. QMAKE overrides the search below (Qt 6 only), for example
#   make QMAKE=/opt/qt6/bin/qmake6
PREFIX ?= /usr/local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
DATADIR ?= $(PREFIX)/share
CARGO ?= cargo
TARGET_DIR ?= $(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),target)
QMAKE ?= $(firstword $(wildcard /usr/lib/qt6/bin/qmake6 /usr/lib64/qt6/bin/qmake6 /usr/lib/qt6/bin/qmake /usr/lib64/qt6/bin/qmake) $(shell command -v qmake6 qmake-qt6 2>/dev/null))
export QMAKE

.PHONY: all build test install uninstall

all: build

build:
	$(CARGO) build --release --locked --workspace

test:
	$(CARGO) test --release --locked --workspace

install: build
	install -Dm755 $(TARGET_DIR)/release/fili-term $(DESTDIR)$(BINDIR)/fili-term
	install -Dm755 $(TARGET_DIR)/release/fili-term-daemon $(DESTDIR)$(BINDIR)/fili-term-daemon
	ln -sf fili-term $(DESTDIR)$(BINDIR)/aeroport
	install -Dm644 data/fili-term.desktop $(DESTDIR)$(DATADIR)/applications/fili-term.desktop
	install -Dm644 data/icons/hicolor/scalable/apps/fili-term.svg $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-term.svg
	install -Dm644 LICENSE $(DESTDIR)$(DATADIR)/licenses/fili-term/LICENSE
	install -Dm644 licenses/JetBrainsMono-OFL.txt $(DESTDIR)$(DATADIR)/licenses/fili-term/JetBrainsMono-OFL.txt
	install -Dm644 licenses/Jost-OFL.txt $(DESTDIR)$(DATADIR)/licenses/fili-term/Jost-OFL.txt

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/fili-term $(DESTDIR)$(BINDIR)/fili-term-daemon $(DESTDIR)$(BINDIR)/aeroport
	rm -f $(DESTDIR)$(DATADIR)/applications/fili-term.desktop
	rm -f $(DESTDIR)$(DATADIR)/icons/hicolor/scalable/apps/fili-term.svg
	rm -rf $(DESTDIR)$(DATADIR)/licenses/fili-term
