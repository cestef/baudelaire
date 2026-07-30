#!/usr/bin/env sh
#
# Build the demo site once per shipped theme, into `docs/public/themes/<name>/`,
# so the docs site can link a live example of each.
#
# Run from the repository root. Both `just previews` and the docs workflow call
# this, rather than either spelling the loop out itself.
#
# Two things decide the shape of this:
#
#   - A theme has to sit inside the project root of the site using it, so the
#     demo is built from the repository root and names `themes/<name>`.
#   - The theme cannot be a `profiles { }` entry: it is resolved when the config
#     loads, before any profile is overlaid, so a theme named in a profile would
#     supply templates and assets but none of its `theme.kdl` defaults. Hence the
#     one appended line rather than three near-identical config files.
#
# It must run *after* the docs site is built: that site sets `prune`, which
# deletes everything under its `dist` that its own build did not produce.
#
# Overridable: BAUDELAIRE (the command to build with), PREVIEW_URL, PREVIEW_OUT.
set -eu

: "${BAUDELAIRE:=cargo run -q --}"
: "${PREVIEW_URL:=https://baudelaire.cstef.dev/themes}"
: "${PREVIEW_OUT:=docs/public/themes}"

for theme in albatros spleen voyage; do
    config="target/previews/$theme.kdl"
    mkdir -p "$(dirname "$config")"
    cp themes/demo/config.kdl "$config"
    printf '\ntheme "themes/%s"\n' "$theme" >>"$config"

    # Unquoted on purpose: BAUDELAIRE carries its own arguments
    # (`cargo run -q --`), and quoting it would look for one such binary.
    # shellcheck disable=SC2086
    $BAUDELAIRE build \
        --config "$config" \
        --out "$PREVIEW_OUT/$theme" \
        --base-url "$PREVIEW_URL/$theme"
done
