#let frontmatter = (
  title: "Atlas",
  date: datetime(year: 2025, month: 9, day: 12),
  summary: "Maps for a warehouse that will not hold still.",
  image: "/assets/atlas.svg",
  alt: "A warehouse floor drawn as blocks on a grid.",
  role: "Interface",
  client: "A logistics firm",
  duration: "Six weeks",
  stack: ("typescript", "canvas"),
)

A warehouse floor changes faster than its drawing. Atlas is a viewer that takes
the day's layout as data and renders it at sixty frames a second on a
three-year-old tablet.

= The trick

Nothing is a DOM node. The floor is one canvas, the labels are drawn, and the
only elements on the page are the controls. That keeps it smooth on old
hardware.

= What shipped

- Pan, zoom, and a search that jumps to a bay.
- A diff view: today's layout against last week's, in one colour.
- An export of the current view as a PNG, which people printed and taped to a
  pillar.
