// The query tokenizer, concatenated ahead of whichever engine a build emits and
// shared with the palette that highlights what a query matched.
//
// One definition, because it has to agree with the build-time tokenizer in
// `search.rs` that keys the inverted index: a document is findable only when a
// query splits the way the index did, and a rule fixed in one of two copies is
// a search that quietly stops matching.
//
// Lowercase first, then strip: `İ`.toLowerCase() is `i` + U+0307, and the mark
// has to be stripped after it appears, not before. And the retained set is
// `\p{Alphabetic}\p{N}`, not `\p{L}\p{N}` -- that is the exact set Rust's
// `char::is_alphanumeric` covers, and it is what keeps the Indic, Arabic and
// Hebrew vowel marks that `\p{L}` alone would drop.

const tokenize = (text) =>
  text.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{Alphabetic}\p{N}]/gu, "")).filter(Boolean);
