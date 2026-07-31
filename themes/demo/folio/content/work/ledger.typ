#let frontmatter = (
  title: "Ledger",
  date: datetime(year: 2026, month: 4, day: 2),
  summary: "A double-entry bookkeeping engine a spreadsheet can read.",
  role: "Design and build",
  client: "Self-directed",
  duration: "Four months",
  stack: ("rust", "sqlite"),
)

Accounting software fails in one of two ways: it is a black box, or it is a
spreadsheet. Ledger is an attempt at neither. Entries go in as plain text, come
out as a queryable database, and export as a sheet somebody's auditor can open
without asking a question.

= The rule it enforces

One rule, checked on write: every transaction sums to zero. Everything else the
engine does is reporting, and reporting is easy once the numbers cannot be
wrong.

```rust
fn post(entries: &[Entry]) -> Result<(), Unbalanced> {
    let sum: i64 = entries.iter().map(|e| e.minor_units).sum();
    (sum == 0).then_some(()).ok_or(Unbalanced(sum))
}
```

Money is integer minor units, never a float, which is the other half of not
being wrong.

= What it refuses to do

No currency conversion at write time. A rate is a fact about a day, and baking
one into a stored entry makes yesterday's books change when today's rate does.
