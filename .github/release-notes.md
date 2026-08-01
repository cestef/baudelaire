<!--
The body of every GitHub release, filled in by the `release notes` step of
`.github/workflows/release.yml`.

A file rather than a heredoc in the workflow, because a heredoc made every
backtick and every `$` in the prose an escaping question, and the notes are
mostly code blocks. Here they are just text.

Three placeholders, substituted by `envsubst` with an explicit list so nothing
else in the page expands:

  ${VERSION}    the tag, e.g. v0.1.0
  ${CHANGES}    this version's section of CHANGELOG.md, verbatim
  ${ARTIFACTS}  one table row per built archive, generated from what was built

`${ARTIFACTS}` is generated on purpose: the table used to name every target by
hand, so adding one to the build matrix meant remembering to add it here too.
-->
## Install

```bash
curl -fsSL https://baudelaire.cstef.dev/install.sh -o install.sh
less install.sh          # read before you run
sh install.sh
```

<details>
<summary>Other ways to install</summary>

```bash
VERSION=${VERSION} sh install.sh    # pin this release
PREFIX=~/.local/bin sh install.sh   # choose the install directory
FLAVOR=slim sh install.sh           # the smaller build, see below
```

With Cargo, `cargo binstall baudelaire` fetches these same archives, and
`cargo install baudelaire` builds from source.

On Windows, unpack `baudelaire-windows-x86_64.zip` and put `baudelaire.exe` on
your `PATH`; the installer script is POSIX-only.

By hand, on Linux or macOS:

```bash
tar -xzf baudelaire-linux-x86_64.tar.gz
install -m 0755 baudelaire ~/.local/bin/
```

The macOS builds are unsigned and unnotarized, so the first run needs a
right-click -> Open, or `xattr -d com.apple.quarantine baudelaire`.

</details>

## Changes

${CHANGES}

## Artifacts

<details>
<summary>Every build, with binary sizes</summary>

Two flavors per target. **full** is the default: everything built in. **slim**
drops the bundled fonts and the CSS/JS/image pipelines, so it renders with
system fonts only and copies `.css`/`.js`/images verbatim. Pick slim when your
host has the fonts and you run your own asset tooling.

The `gnu` builds link the host's glibc; the fully static `musl` builds run
anywhere (Alpine, distroless, scratch containers).

| Artifact | Binary |
|----------|--------|
${ARTIFACTS}

Every archive ships a matching `.sha256`, the installer too. It is fetched from
the same origin as the archive, so it proves the transfer was not corrupted; it
is not a signature and does not prove authenticity.

```bash
sha256sum -c baudelaire-linux-x86_64.tar.gz.sha256   # shasum -a 256 -c on macOS
```

</details>
