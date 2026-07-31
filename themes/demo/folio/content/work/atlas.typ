#let frontmatter = (
  title: "Atlas",
  date: datetime(year: 2025, month: 9, day: 12),
  summary: "Maps for a warehouse that will not hold still.",
  role: "Interface",
  client: "A logistics firm",
  duration: "Six weeks",
  stack: ("typescript", "canvas"),
)

A warehouse floor changes faster than the drawing of it. Atlas is a viewer that
takes the day's layout as data and renders it at sixty frames a second on the
sort of tablet that is already three years old.

= The trick

Nothing is a DOM node. The floor is one canvas, the labels are drawn, and the
only elements on the page are the controls. That is why it stays smooth on
hardware everyone else had given up on.

= What shipped

- Pan, zoom, and a search that jumps to a bay.
- A diff view: today's layout against last week's, in one colour.
- An export nobody asked for and everybody used: the current view as a PNG,
  printed and taped to a pillar.
