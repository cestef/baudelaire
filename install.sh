#!/bin/sh
# Baudelaire installer — a prebuilt, checksum-verified binary onto your PATH.
# No Rust needed. Read it before you run it:
#   curl -fsSL https://baudelaire.dev/install.sh | less
#
# Knobs:  VERSION=v0.1.0   PREFIX=/usr/local/bin   sh install.sh
set -eu

REPO="${REPO:-https://codeberg.org/cstef/baudelaire}"
PREFIX="${PREFIX:-$HOME/.local/bin}"
VERSION="${VERSION:-}"

# Colors, on a terminal only.
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  b=$(printf '\033[1m'); d=$(printf '\033[2m'); p=$(printf '\033[35m')
  g=$(printf '\033[32m'); r=$(printf '\033[31m'); x=$(printf '\033[0m')
else b= d= p= g= r= x=; fi
step() { printf '%s→%s %s\n' "$p" "$x" "$1"; }
die()  { printf '%s✗%s %s\n' "$r" "$x" "$1" >&2; exit 1; }

# One downloader, whichever is present.
if command -v curl >/dev/null 2>&1; then
  dl() { curl -fsSL "$1" 2>/dev/null; }; dlf() { curl -fsSL -o "$2" "$1" 2>/dev/null; }
elif command -v wget >/dev/null 2>&1; then
  dl() { wget -qO- "$1" 2>/dev/null; }; dlf() { wget -qO "$2" "$1" 2>/dev/null; }
else die "need ${b}curl${x} or ${b}wget${x}"; fi

# Platform — prebuilts are Linux x86_64 / aarch64; else build from source.
source="build from source: ${b}cargo install --git $REPO${x}"
[ "$(uname -s)" = Linux ] || die "prebuilt binaries are Linux-only — $source"
case $(uname -m) in
  x86_64 | amd64)  arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported arch $(uname -m) — $source" ;;
esac

# Newest release, unless pinned.
if [ -z "$VERSION" ]; then
  step "resolving latest release"
  VERSION=$(dl "$REPO/api/v1/repos/cstef/baudelaire/releases/latest" \
    | tr ',' '\n' | sed -n 's/.*"tag_name":"\([^"]*\)".*/\1/p' | head -1)
  [ -n "$VERSION" ] || die "couldn't resolve latest tag — pin one with ${b}VERSION=${x}"
fi

# Download, verify, install.
asset="baudelaire-linux-$arch.tar.gz"
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
step "downloading ${b}baudelaire $VERSION${x} ${d}(linux-$arch)${x}"
dlf "$REPO/releases/download/$VERSION/$asset" "$tmp/$asset" || die "download failed"
dlf "$REPO/releases/download/$VERSION/$asset.sha256" "$tmp/$asset.sha256" || die "checksum download failed"

step "verifying checksum"
sum=sha256sum; command -v sha256sum >/dev/null 2>&1 || sum="shasum -a 256"
( cd "$tmp" && $sum -c "$asset.sha256" >/dev/null 2>&1 ) || die "checksum mismatch — aborting"

tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$PREFIX"
install -m 0755 "$tmp/baudelaire" "$PREFIX/baudelaire" || die "install to $PREFIX failed"

printf '%s✓%s installed %sbaudelaire %s%s → %s%s%s\n' \
  "$g" "$x" "$b" "$VERSION" "$x" "$b" "$PREFIX/baudelaire" "$x"
case ":$PATH:" in
  *":$PREFIX:"*) printf '  %sbaudelaire init%s to scaffold a site\n' "$b" "$x" ;;
  *) printf '  %snot on PATH%s — add: %sexport PATH="%s:$PATH"%s\n' "$d" "$x" "$b" "$PREFIX" "$x" ;;
esac
