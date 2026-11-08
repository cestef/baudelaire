#let frontmatter = (
  title: "Typst est le gabarit",
  date: datetime(year: 2026, month: 7, day: 12),
  tags: ("typst", "gabarits"),
  summary: "Pourquoi le contenu et la mise en page doivent parler une seule langue.",
)

La plupart des générateurs demandent de tenir deux langages à la fois : un pour
la prose, un pour le balisage autour. La gêne vient de la couture entre les
deux.

= Une seule langue

Ici, pas de couture. Un titre est un titre, et la mise en page qui l'entoure
s'écrit dans la même langue, avec les mêmes fonctions :

```typ
#let page(page, body) = h("article", {
  h("h1", page.frontmatter.title)
  body
})
```

Ce gabarit construit de vrais éléments au lieu d'assembler une chaîne. Un
attribut avec un tiret, un rôle ARIA ou un `<svg>` en ligne ne demande donc
aucune échappatoire.

#quote(block: true, attribution: [le README])[
  Les pages, les mises en page et les données parlent une seule langue, donc un
  titre, une note de bas de page et un menu s'écrivent de la même façon.
]
