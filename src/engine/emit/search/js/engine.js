// The one search engine, over either shape of a baudelaire index. A `terms`
// index arrives with its postings built; a `documents` index arrives as prose
// and is indexed here under the rules its header carries. Everything after that
// point -- prefix expansion, scoring, ranking -- is the same code, so a site
// changes shape without changing what a query finds.
//
// Concatenated with tokenize.js and palette.js into one module scope, and
// served as the `baudelaire:search` virtual module. `INDEXES` and `LANG` come
// from the generated prelude.

const EMPTY = { docs: [], terms: [], postings: [], exact: new Map(), base: "", snippet: 0 };

// How much a prefix match is worth against a whole-word one, and how many terms
// one prefix may expand to: a two-letter query otherwise drags in the whole
// index.
const PREFIX_FACTOR = 0.5;
const PREFIX_LIMIT = 64;

const indexUrl = () => {
  const lang = (typeof document !== "undefined" && document.documentElement.lang) || LANG;
  return INDEXES[lang] || INDEXES[LANG];
};

// Postings for a `documents` index, built exactly as the build would have built
// them: same tokenizer, same weights, same stopwords, same minimum.
const indexDocuments = (data) => {
  const weights = data.weights || {};
  const stop = new Set(data.stopwords || []);
  const scores = new Map();
  data.documents.forEach((doc, id) => {
    const fields = [
      [doc.title || "", weights.title],
      [doc.text || "", weights.body],
      ...(doc.tags || []).map((tag) => [tag, weights.tags]),
    ];
    for (const [text, weight] of fields) {
      if (!weight) continue;
      for (const token of tokenize(text)) {
        if ([...token].length < (data.minimum || 0) || stop.has(token)) continue;
        let carrying = scores.get(token);
        if (!carrying) scores.set(token, (carrying = new Map()));
        carrying.set(id, (carrying.get(id) || 0) + weight);
      }
    }
  });
  const terms = [...scores.keys()].sort();
  return { terms, postings: terms.map((term) => [...scores.get(term)]) };
};

const build = (data) => {
  const { terms, postings } = data.index === "terms" ? data : indexDocuments(data);
  return {
    docs: data.documents || [],
    terms,
    postings,
    // Exact lookup goes through a map, not the binary search: the terms are
    // sorted by the build in code-point order, which is not the order JavaScript
    // compares strings in above the basic plane.
    exact: new Map(terms.map((term, at) => [term, at])),
    base: data.base || "",
    snippet: data.snippet || 0,
  };
};

// The first term at or after `prefix`, by binary search over the sorted terms.
const lowerBound = (terms, prefix) => {
  let [low, high] = [0, terms.length];
  while (low < high) {
    const mid = (low + high) >> 1;
    if (terms[mid] < prefix) low = mid + 1;
    else high = mid;
  }
  return low;
};

// Every indexed term a query term reaches, as `[position, factor]`: itself, and
// -- for the term still being typed -- what it is a prefix of.
const reach = (index, term, typing) => {
  const found = [];
  const at = index.exact.get(term);
  if (at !== undefined) found.push([at, 1]);
  if (!typing) return found;
  for (let i = lowerBound(index.terms, term); i < index.terms.length; i++) {
    if (!index.terms[i].startsWith(term)) break;
    if (found.length > PREFIX_LIMIT) break;
    if (index.terms[i] !== term) found.push([i, PREFIX_FACTOR]);
  }
  return found;
};

// Documents ranked by how many of the query's terms they carry first, by score
// second: an AND-leaning order that still answers a query one term of which is
// a typo.
const rank = (index, query, limit) => {
  const terms = tokenize(query);
  if (terms.length === 0) return [];
  const totals = new Map();
  terms.forEach((term, i) => {
    const scored = new Map();
    for (const [at, factor] of reach(index, term, i === terms.length - 1)) {
      for (const [doc, score] of index.postings[at]) {
        scored.set(doc, Math.max(scored.get(doc) || 0, score * factor));
      }
    }
    for (const [doc, score] of scored) {
      const seen = totals.get(doc) || { score: 0, matched: 0 };
      totals.set(doc, { score: seen.score + score, matched: seen.matched + 1 });
    }
  });
  return [...totals.entries()]
    .sort((a, b) => b[1].matched - a[1].matched || b[1].score - a[1].score)
    .slice(0, limit)
    .map(([doc]) => index.docs[doc])
    .filter(Boolean);
};

// A searcher over one index: `search(query, { limit })`, carrying the base path
// hits are linked under and the snippet length the index was written for.
export async function createSearch(url) {
  const from = url || indexUrl();
  let index = EMPTY;
  let failed = false;
  try {
    const data = await fetch(from).then((response) => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.json();
    });
    index = build(data);
  } catch (error) {
    // Never silent: an unreachable or malformed index otherwise looks like a
    // site with no results. The palette reads `search.failed` to say so.
    failed = true;
    console.warn(`baudelaire: search index ${from} failed to load:`, error);
  }
  const search = function search(query, { limit = 12 } = {}) {
    return rank(index, query, limit);
  };
  search.failed = failed;
  search.base = index.base;
  search.snippet = index.snippet;
  return search;
}
