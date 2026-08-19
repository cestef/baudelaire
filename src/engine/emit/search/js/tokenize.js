// The query tokenizer, concatenated ahead of the engine and shared with the
// palette that highlights what a query matched.
//
// One definition, because it has to agree with `Tokens::normalize` in
// `tokens.rs`, which keys the index: a document is findable only when a query
// splits the way the index did, and a rule fixed in one of two copies is a
// search that quietly stops matching.
//
// The set of characters a key keeps is `KEPT`, from the generated prelude:
// `Tokens::KEPT` in tokens.rs writes it, so neither side can narrow it alone.
// The order is this file's own, and matters: lowercase first, then strip, since
// `İ`.toLowerCase() is `i` + U+0307 and the mark has to be stripped after it
// appears, not before.

const strip = new RegExp(`[^${KEPT}]`, "gu");

const tokenize = (text) =>
  text.toLowerCase().split(/\s+/).map((w) => w.replace(strip, "")).filter(Boolean);
