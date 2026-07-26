// Search engine for the prebuilt `search.inverted.json` index
// ({ documents, postings }): resolves a query by term lookup and posting-list
// intersection, no full scan. Documents carry only url/title (no body), so hits
// have no snippet. Concatenated with palette.js to form the generated client;
// also served as the `baudelaire:search/inverted` virtual module. Exposes the
// same `createSearch` contract as the flat engine.

const tokenize = (text) =>
  text.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}]/gu, "")).filter(Boolean);

export async function createSearch(url = INDEX) {
  let documents = [];
  let postings = {};
  let failed = false;
  try {
    const index = await fetch(url).then((response) => {
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      return response.json();
    });
    documents = index.documents || [];
    postings = index.postings || {};
  } catch (error) {
    // Never silent: an unreachable or malformed index otherwise looks like a
    // site with no results. The palette reads `search.failed` to say so.
    failed = true;
    console.warn(`baudelaire: search index ${url} failed to load:`, error);
  }

  const search = function search(query, { limit = 12 } = {}) {
    const terms = tokenize(query);
    if (terms.length === 0) return [];

    // Rank by how many distinct query terms a document matches (AND-leaning).
    const matches = new Map();
    for (const term of terms) {
      for (const id of postings[term] || []) {
        matches.set(id, (matches.get(id) || 0) + 1);
      }
    }

    return [...matches.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, limit)
      .map(([id]) => documents[id])
      .filter(Boolean);
  };
  search.failed = failed;
  return search;
}
