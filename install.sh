#!/bin/sh
# baudelaire installer: a prebuilt, checksum-verified binary onto your path.
set -eu

REPO="${REPO:-https://codeberg.org/cstef/baudelaire}"
PREFIX="${PREFIX:-$HOME/.local/bin}"
VERSION="${VERSION:-}"
FLAVOR="${FLAVOR:-full}" # full, or slim: only system fonts + assets copied as-is

# colors
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  b=$(printf '\033[1m'); d=$(printf '\033[2m'); p=$(printf '\033[35m')
  g=$(printf '\033[32m'); r=$(printf '\033[31m'); x=$(printf '\033[0m')
else b= d= p= g= r= x=; fi
step() { printf '%s→%s %s\n' "$p" "$x" "$1"; }
die()  { printf '%s✗%s %s\n' "$r" "$x" "$1" >&2; exit 1; }

if command -v curl >/dev/null 2>&1; then
  dl() { curl -fsSL -o "$2" "$1" 2>/dev/null; }
elif command -v wget >/dev/null 2>&1; then
  dl() { wget -qO "$2" "$1" 2>/dev/null; }
else die "need ${b}curl${x} or ${b}wget${x}"; fi

# linux x86_64/aarch64 only
src="build from source: ${b}cargo install --git $REPO${x}"
[ "$(uname -s)" = Linux ] || die "prebuilt binaries are linux-only. $src"
case $(uname -m) in
  x86_64 | amd64)  arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported arch $(uname -m). $src" ;;
esac

case "$FLAVOR" in
  full) suffix= ;;
  slim) suffix=-slim ;;
  *) die "unknown ${b}FLAVOR=$FLAVOR${x}, use ${b}full${x} or ${b}slim${x}" ;;
esac

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

# latest tag
if [ -z "$VERSION" ]; then
  step "resolving latest release"
  dl "$REPO/api/v1/repos/cstef/baudelaire/releases/latest" "$tmp/rel" || die "release lookup failed"
  VERSION=$(tr ',' '\n' < "$tmp/rel" | sed -n 's/.*"tag_name":"\([^"]*\)".*/\1/p' | head -1)
  [ -n "$VERSION" ] || die "couldn't resolve latest tag, pin one with ${b}VERSION=${x}"
fi

# download tar + sha
asset="baudelaire-linux-$arch$suffix.tar.gz"
step "downloading ${b}baudelaire $VERSION${x} ${d}(linux-$arch, $FLAVOR)${x}"
for f in "$asset" "$asset.sha256"; do
  dl "$REPO/releases/download/$VERSION/$f" "$tmp/$f" || die "download failed: $f"
done

# verify + install
step "verifying checksum"
sum=sha256sum; command -v sha256sum >/dev/null 2>&1 || sum="shasum -a 256"
( cd "$tmp" && $sum -c "$asset.sha256" >/dev/null 2>&1 ) || die "checksum mismatch, aborting"
tar -xzf "$tmp/$asset" -C "$tmp"
mkdir -p "$PREFIX"
install -m 0755 "$tmp/baudelaire" "$PREFIX/baudelaire" || die "install to $PREFIX failed"

printf '%s✓%s installed %sbaudelaire %s%s → %s%s%s\n' \
  "$g" "$x" "$b" "$VERSION" "$x" "$b" "$PREFIX/baudelaire" "$x"
case ":$PATH:" in
  *":$PREFIX:"*) printf '  %sbaudelaire init%s to scaffold a site\n' "$b" "$x" ;;
  *) printf '  %snot on PATH%s, add: %sexport PATH="%s:$PATH"%s\n' "$d" "$x" "$b" "$PREFIX" "$x" ;;
esac
