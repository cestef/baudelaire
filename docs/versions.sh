#!/usr/bin/env sh
#
# Build every published version of the docs site into `docs/public/`.
#
# Run from the repository root. Both `just versions` and the docs workflow call
# this, rather than either spelling the loop out itself. `docs/versions.sh list`
# prints the published tags and does nothing else, which is how the fast local
# docs loop refreshes the picker's list without building anything.
#
# A version of the docs is built from that tag's own `docs/` tree, by that tag's
# own released binary, downloaded with `install.sh`. Nothing else is honest: the
# docs describe a binary, so the binary that renders them has to be the one they
# describe. Building an old tree with a new binary would document features it
# never had, and building a new tree with an old binary would fail on config
# keys that did not exist yet.
#
# The layout it produces:
#
#   public/            the newest published version, at the site root
#   public/vX.Y.Z/     every published version, pinned, newest included
#   public/latest/     nothing; `_redirects` sends it to the root
#
# The root and the newest pinned copy are the same tree built twice, under two
# base URLs. The pinned copy is what a link written today still resolves to
# after the next release.
#
# The root build prunes, so everything else under `public/` survives only
# because `docs/config.kdl` names it in `prune { keep }`. That is what makes the
# order this runs in relative to `themes/demo/build.sh` a preference rather than
# a rule; before the keep list existed it was a rule, and an unwritten one.
#
# Overridable: SITE_URL, OUT, WORK (scratch for worktrees and binaries), WINDOW.
set -eu

: "${SITE_URL:=https://baudelaire.cstef.dev}"
: "${OUT:=docs/public}"
: "${WORK:=target/docs-versions}"
: "${WINDOW:=3}"

# What is published, derived rather than listed: `CHANGELOG.md` is where a
# release is declared (`.github/workflows/release.yml` refuses a tag with no
# section of its own), and a tag is what proves it happened. Intersecting the
# two is the whole definition, so there is no list to forget to update.
#
# `WINDOW` is the only editorial number here: how far back the site keeps
# serving. Everything older is a tag you can still install, with no docs of its
# own on the site.
published() {
    sed -n 's/^## \[\([0-9][0-9.]*\)\].*/v\1/p' CHANGELOG.md | while read -r tag; do
        git rev-parse -q --verify "refs/tags/$tag" >/dev/null && echo "$tag"
    done | head -"$WINDOW"
}

# The picker in `docs/templates/theme.typ` reads this, so it offers exactly the
# versions that exist. Committed, because the docs site is a real baudelaire
# site and `baudelaire serve` in `docs/` has to work on a fresh clone; rewritten
# on every build, because a stale copy would offer a version that is gone.
write_list() {
    _tree=$1
    shift
    # `mkdir -p` because a tag old enough to predate `docs/generated/` still
    # gets the list, harmlessly: its own frozen theme has no picker to read it.
    mkdir -p "$_tree/docs/generated"
    printf '%s\n' "$@" > "$_tree/docs/generated/versions.csv"
}

versions=$(published)
# Above the `list` branch, not below it: `just versions-list` redirects that
# output over the checked-in file, so an empty answer here (a shallow clone, a
# fork with no tags) would truncate it and exit 0, and the picker would render
# an empty array.
[ -n "$versions" ] || { echo "no released version found in CHANGELOG.md" >&2; exit 1; }

if [ "${1:-}" = list ]; then
    printf '%s\n' "$versions"
    exit 0
fi

root=$(pwd)
abs() { case $1 in /*) printf '%s' "$1" ;; *) printf '%s/%s' "$root" "$1" ;; esac; }
out=$(abs "$OUT")
work=$(abs "$WORK")

rm -rf "$work"
mkdir -p "$work"
# The scratch tree just went away, so every worktree git still knows about is a
# stale registration that would refuse the path it is about to be given again.
git worktree prune

# One worktree and one binary per version, both keyed by tag. `--detach` because
# a tag is not a branch, and `--force` because a worktree left by an interrupted
# run would otherwise refuse the path.
#
# A tag whose release is not downloadable yet is skipped rather than fatal. The
# release commit is pushed with its tag, so the docs run it triggers can start
# before the release workflow has finished uploading binaries. Skipping means
# that run publishes the previous version and the `release: published` trigger
# rebuilds with the new one minutes later.
ready=
for tag in $versions; do
    echo "--> $tag: binary and worktree"
    if PREFIX="$work/bin/$tag" VERSION="$tag" sh install.sh >/dev/null 2>&1; then
        git worktree add --detach --force "$work/tree/$tag" "$tag" >/dev/null
        ready="$ready $tag"
    else
        echo "    no downloadable release yet, skipping" >&2
    fi
done

# The surviving versions become the positional parameters, which is `sh`'s only
# real list: newest first, `$1` the one the site root serves.
# shellcheck disable=SC2086
set -- $ready
[ $# -gt 0 ] || { echo "no published version has a downloadable release" >&2; exit 1; }
versions=$ready
latest=$1

# Into every worktree, not just this one: the root site is built from the tag's
# checkout, so the list committed at that tag is what its picker would otherwise
# show, and that list cannot know about a release that came after it.
write_list "$root" "$@"
for tag in $versions; do
    write_list "$work/tree/$tag" "$@"
done

# The root first: it prunes, so every versioned build has to land after it.
echo "--> $latest: site root"
(cd "$work/tree/$latest/docs" && "$work/bin/$latest/baudelaire" build --out "$out")

# Cloudflare Pages deploy files, written after the root build because that build
# prunes everything under `public/` it did not produce itself.
#
# `/latest/` is the URL that never goes stale. The pinned copies duplicate the
# root's pages word for word, so keeping them out of search leaves one result
# per page while a link someone pinned still resolves.
echo "/latest/*  /:splat  302" > "$out/_redirects"
for tag in $versions; do
    echo "/$tag/*"
    echo "  X-Robots-Tag: noindex"
done > "$out/_headers"

# The banner every pinned copy carries, since the theme that rendered it is
# frozen and cannot grow a picker. Injected into the built HTML rather than
# authored, which is the one place in this repository that rewrites markup as a
# string: the alternative is overlaying today's templates onto an old tree,
# where any template using a config key or module newer than that tag breaks the
# build.
#
# Below the header, not above it: a frozen theme's CSS was written for a header
# that starts at the top of the page, and the header is positioned, so it is the
# containing block for anything absolute inside it. Stacking the banner on top
# shifted the header down and dragged the skip link, parked just above it, into
# view. Going under the header disturbs nothing, in trees whose CSS this cannot
# reach either.
#
# `|` delimits the substitution, so nothing here may contain one, and `&` is the
# whole match in a sed replacement, so nothing here may contain one of those
# either.
banner() {
    # `sh` has no local scope, so these are spelled apart from the loop
    # variables they would otherwise clobber.
    _tag=$1
    _href=$2
    style="display:flex;gap:.4rem;justify-content:center;flex-wrap:wrap;padding:.55rem 1rem;margin:0;font:500 .85rem/1.4 ui-sans-serif,system-ui,sans-serif;color:#422006;background:#fde68a;border-bottom:1px solid #d97706"
    link="color:inherit;text-decoration:underline"
    if [ "$_tag" = "$latest" ]; then
        text="Pinned copy of $_tag.<a href=\"$_href\" style=\"$link\">The current docs live here</a>."
    else
        text="Docs for $_tag.<a href=\"$_href\" style=\"$link\">$latest is the current release</a>."
    fi
    printf '<div role="note" style="%s">%s</div>' "$style" "$text"
}

for tag in $versions; do
    echo "--> $tag: pinned copy"
    dir=$out/$tag
    (cd "$work/tree/$tag/docs" &&
        "$work/bin/$tag/baudelaire" build --base-url "$SITE_URL/$tag/" --out "$dir")

    # The banner links to the same page at the root, so a reader who landed on
    # an old page keeps their place when they take the offer. A page that no
    # longer exists there lands on the root's 404, which Cloudflare Pages serves
    # for every miss on the site including these subpaths.
    find "$dir" -name '*.html' | while read -r file; do
        rel=${file#"$dir/"}
        case $rel in
            index.html) href=/ ;;
            */index.html) href=/${rel%index.html} ;;
            *) href=/$rel ;;
        esac
        # Every page of every published tree closes exactly one `header`, so a
        # plain global substitution lands the banner once.
        sed "s|</header>|</header>$(banner "$tag" "$href")|" "$file" > "$file.tmp"
        mv "$file.tmp" "$file"
    done
done

# The checkouts are scratch, and leaving them registered would put one detached
# entry per version in `git worktree list` until the next run.
rm -rf "$work/tree"
git worktree prune

echo "--> built $# versions into $OUT"
