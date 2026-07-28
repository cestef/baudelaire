// The query tokenizer, concatenated ahead of whichever engine a build emits and
// shared with the palette that highlights what a query matched.
//
// One definition, because it has to agree with the build-time tokenizer in
// `search.rs` that keys the inverted index: a document is findable only when a
// query splits the way the index did, and a rule fixed in one of two copies is
// a search that quietly stops matching.

const tokenize = (text) =>
  text.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}]/gu, "")).filter(Boolean);
