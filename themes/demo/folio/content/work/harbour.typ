#let frontmatter = (
  title: "Harbour",
  date: datetime(year: 2024, month: 11, day: 20),
  summary: "A deploy tool that asks one question and then stops talking.",
  role: "Build",
  duration: "Ongoing",
  stack: ("rust",),
)

Every deploy tool grows a dashboard. Harbour has a prompt: it prints what it is
about to change, waits, and then does exactly that. No plan file, no drift
report, no second source of truth about what is running.

= Why one question

Because the answer to "what will this do" has to fit on a screen. If the diff is
too long to read, the change is too large to make, and the tool saying so is
more useful than the tool doing it.

= Ongoing

Still in use, still under a hundred kilobytes, still without a dashboard.
