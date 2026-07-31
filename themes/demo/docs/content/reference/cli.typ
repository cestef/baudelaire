#let frontmatter = (
  title: "Command line",
  order: 1,
  summary: "Every command, and the flags worth knowing.",
  tags: ("reference",),
)

#table(
  columns: 2,
  table.header[Command][What it does],
  [`wren build`], [build the thing once],
  [`wren serve`], [build it, watch it, rebuild it],
  [`wren check`], [say what is wrong without writing anything],
  [`wren publish`], [put it somewhere],
)

= Flags

/ `--config <path>`: read another config file.
/ `--out <dir>`: write somewhere other than the configured output.
/ `--quiet`: print nothing that is not an error.

= Exit codes

`0` for success, `1` for a failed build, `2` for a config that could not be
read. Anything else is a bug, in a tool that does not exist, which makes it hard
to report.
