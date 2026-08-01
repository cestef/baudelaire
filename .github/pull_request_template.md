<!--
Thanks for the patch. The checklist below is what CI checks anyway; going
through it first turns a review round-trip into a merge.
-->

## What this changes

<!-- One paragraph. If it fixes an issue, "Fixes #123". -->

## Why

<!--
What was wrong or missing. For anything non-obvious, the reasoning belongs in a
comment in the code as well: this repo comments why, not what.
-->

## Checklist

- [ ] `just ci` passes (both feature flavors, not just the default one)
- [ ] One concern per commit, and each commit builds and passes on its own
- [ ] Conventional Commits subject, one line, no body
- [ ] `CHANGELOG.md` updated under `## [Unreleased]`, if this is user-facing
- [ ] A breaking change carries its migration inline: the exact config line,
      flag spelling, or import that restores the old behaviour
- [ ] `just docs` still builds, if this touched `docs/`, `themes/`, or output
- [ ] New behaviour has a test; a bug fix has one that fails without the fix
