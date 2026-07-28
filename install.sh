#!/bin/sh
# baudelaire installer: a prebuilt, checksum-verified binary onto your path.
set -eu

REPO="${REPO:-https://github.com/cestef/baudelaire}"
# Release metadata lives on a different host than the release downloads, so the
# API base is its own knob rather than a suffix on $REPO.
API="${API:-https://api.github.com/repos/cestef/baudelaire}"
PREFIX="${PREFIX:-$HOME/.local/bin}"
VERSION="${VERSION:-}"
FLAVOR="${FLAVOR:-full}" # full, or slim: only system fonts + assets copied as-is
LIBC="${LIBC:-}"        # gnu or musl; empty auto-detects

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

# linux x86_64/aarch64, macos on apple silicon
src="build from source: ${b}cargo install --git $REPO${x}"
case $(uname -s) in
  Linux)  os=linux ;;
  Darwin) os=macos ;;
  *) die "no prebuilt binary for $(uname -s). $src" ;;
esac
case $(uname -m) in
  x86_64 | amd64)  arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) die "unsupported arch $(uname -m). $src" ;;
esac
# No Intel mac builds: the runner image for them retires in 2027 and the
# hardware is nearly gone. Source builds still work.
[ "$os$arch" = macosx86_64 ] && die "no prebuilt binary for Intel Macs. $src"

# The libc suffix, on Linux only: macOS ships one libc and one set of assets.
# musl build on musl hosts (Alpine), glibc otherwise. LIBC= overrides.
libc=
if [ "$os" = linux ]; then
  if [ -z "$LIBC" ]; then
    LIBC=gnu
    for ld in /lib/ld-musl-*.so.1; do
      [ -e "$ld" ] && { LIBC=musl; break; }
    done
    [ "$LIBC" = gnu ] && ldd --version 2>&1 | grep -qi musl && LIBC=musl
  fi
  case "$LIBC" in
    gnu)  libc= ;;
    musl) libc=-musl ;;
    *) die "unknown ${b}LIBC=$LIBC${x}, use ${b}gnu${x} or ${b}musl${x}" ;;
  esac
fi

case "$FLAVOR" in
  full) suffix= ;;
  slim) suffix=-slim ;;
  *) die "unknown ${b}FLAVOR=$FLAVOR${x}, use ${b}full${x} or ${b}slim${x}" ;;
esac

tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT

# latest tag
if [ -z "$VERSION" ]; then
  step "resolving latest release"
  dl "$API/releases/latest" "$tmp/rel" || die "release lookup failed"
  VERSION=$(tr ',' '\n' < "$tmp/rel" | sed -n 's/.*"tag_name"[ 	]*:[ 	]*"\([^"]*\)".*/\1/p' | head -1)
  [ -n "$VERSION" ] || die "couldn't resolve latest tag, pin one with ${b}VERSION=${x}"
fi

# download tar + sha
asset="baudelaire-$os-$arch$libc$suffix.tar.gz"
step "downloading ${b}baudelaire $VERSION${x} ${d}($os-$arch$libc, $FLAVOR)${x}"
for f in "$asset" "$asset.sha256"; do
  dl "$REPO/releases/download/$VERSION/$f" "$tmp/$f" || die "download failed: $f"
done

# verify + install
# The checksum is fetched from the same origin as the tarball, so it proves the
# transfer was not truncated or corrupted; it is not a signature and does not
# prove authenticity.
step "verifying checksum"
if command -v sha256sum >/dev/null 2>&1; then sum="sha256sum"
elif command -v shasum >/dev/null 2>&1; then sum="shasum -a 256"
else die "no ${b}sha256sum${x} or ${b}shasum${x} found, cannot verify the download"
fi
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
