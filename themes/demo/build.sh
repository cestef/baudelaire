#!/usr/bin/env sh
#
# Build a demo site per shipped theme, into `docs/public/themes/<name>/`, so the
# docs site can link a live example of each.
#
# Run from the repository root. Both `just previews` and the docs workflow call
# this, rather than either spelling the loop out itself.
#
# Each theme names the demo whose content it was designed for: a documentation
# theme showing blog posts, or a portfolio theme with no projects in it, proves
# nothing. The sets live in `themes/demo/<set>/`, and more than one theme may
# share one.
#
# Two things decide the shape of this:
#
#   - A theme has to sit inside the project root of the site using it, so the
#     demo is built from the repository root and names `themes/<name>`.
#   - The theme cannot be a `profiles { }` entry: it is resolved when the config
#     loads, before any profile is overlaid, so a theme named in a profile would
#     supply templates and assets but none of its `theme.kdl` defaults. Hence the
#     one appended line rather than a near-identical config file per theme.
#
# It must run *after* the docs site is built: that site sets `prune`, which
# deletes everything under its `dist` that its own build did not produce.
#
# Overridable: BAUDELAIRE (the command to build with), PREVIEW_URL, PREVIEW_OUT.
set -eu

: "${BAUDELAIRE:=cargo run -q --}"
: "${PREVIEW_URL:=https://baudelaire.cstef.dev/themes}"
: "${PREVIEW_OUT:=docs/public/themes}"

# `<theme> <demo set>`, the single mapping: adding a theme is one line here.
for pair in "albatros blog" "spleen blog" "phares docs" "paysage folio"; do
    # shellcheck disable=SC2086
    set -- $pair
    theme="$1"
    demo="$2"

    config="target/previews/$theme.kdl"
    mkdir -p "$(dirname "$config")"
    cp "themes/demo/$demo/config.kdl" "$config"
    printf '\ntheme "themes/%s"\n' "$theme" >>"$config"

    # Unquoted on purpose: BAUDELAIRE carries its own arguments
    # (`cargo run -q --`), and quoting it would look for one such binary.
    # shellcheck disable=SC2086
    $BAUDELAIRE build \
        --config "$config" \
        --out "$PREVIEW_OUT/$theme" \
        --base-url "$PREVIEW_URL/$theme"
done
