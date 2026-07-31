#let frontmatter = (
  title: "Typst est le gabarit",
  date: datetime(year: 2026, month: 7, day: 12),
  tags: ("typst", "gabarits"),
  summary: "Pourquoi le fait que le contenu et la mise en page parlent une seule langue change tout.",
)

La plupart des générateurs demandent de tenir deux langages à la fois : un pour
la prose, un pour le balisage autour. C'est à la couture entre les deux que la
gêne apparaît.

= Une seule langue

Ici, il n'y a pas de couture. Un titre est un titre, et la mise en page qui
l'entoure s'écrit dans la même langue, avec les mêmes fonctions :

```typ
#let page(page, body) = h("article", {
  h("h1", page.frontmatter.title)
  body
})
```

Ce gabarit n'assemble pas une chaîne de caractères. Il construit de vrais
éléments, et c'est pourquoi un attribut avec un tiret, un rôle ARIA ou un
`<svg>` en ligne ne demande aucune échappatoire.

#quote(block: true, attribution: [le README])[
  Vos pages, vos mises en page et vos données parlent une seule langue.
]
