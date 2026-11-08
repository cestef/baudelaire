#let frontmatter = (
  title: "Ledger",
  date: datetime(year: 2026, month: 4, day: 2),
  summary: "A double-entry bookkeeping engine a spreadsheet can read.",
  image: "/assets/ledger.svg",
  alt: "Ruled columns of figures.",
  role: "Design and build",
  client: "Self-directed",
  duration: "Four months",
  stack: ("rust", "sqlite"),
)

Accounting software is usually either a black box or a spreadsheet. Ledger is
neither. Entries go in as plain text, come out as a queryable database, and
export as a sheet an auditor can open without asking a question.

= The rule it enforces

One rule, checked on write: every transaction sums to zero. Everything else the
engine does is reporting.

```rust
fn post(entries: &[Entry]) -> Result<(), Unbalanced> {
    let sum: i64 = entries.iter().map(|e| e.minor_units).sum();
    (sum == 0).then_some(()).ok_or(Unbalanced(sum))
}
```

Money is integer minor units, never a float.

= What it refuses to do

No currency conversion at write time. A rate is a fact about one day, and baking
it into a stored entry changes yesterday's books when today's rate changes.
