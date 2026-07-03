// Search engine for the flat `search.json` index: a document array carrying
// title/body/tags. Ranks at query time, so hits keep their body for snippets.
// Concatenated with palette.js to form the generated client; also served as the
// `baudelaire:search` virtual module. Exposes `createSearch`, which the palette
// (or a library user) calls to get a `search(query, { limit })` function.

const tokenize = (text) =>
  text.toLowerCase().split(/\s+/).map((w) => w.replace(/[^\p{L}\p{N}]/gu, "")).filter(Boolean);

export async function createSearch(url = "/search.json") {
  let documents = [];
  try {
    documents = await fetch(url).then((response) => response.json());
  } catch {
    documents = [];
  }

  return function search(query, { limit = 12 } = {}) {
    const terms = tokenize(query);
    if (terms.length === 0) return [];
    const phrase = query.trim().toLowerCase();
    const scored = [];

    for (const doc of documents) {
      const title = (doc.title || "").toLowerCase();
      const tags = (doc.tags || []).join(" ").toLowerCase();
      const body = (doc.body || "").toLowerCase();
      let score = 0;
      let matchesAll = true;

      for (const term of terms) {
        const inTitle = title.includes(term);
        const inTags = tags.includes(term);
        const inBody = body.includes(term);
        if (!inTitle && !inTags && !inBody) {
          matchesAll = false;
          break;
        }
        if (inTitle) score += title.startsWith(term) ? 20 : 10; // titles dominate
        if (inTags) score += 6;
        if (inBody) score += 2;
      }

      if (!matchesAll) continue;
      // Exact multi-word phrase in title or body earns a bonus.
      if (terms.length > 1 && (title.includes(phrase) || body.includes(phrase))) score += 15;
      scored.push({ doc, score });
    }

    return scored.sort((a, b) => b.score - a.score).slice(0, limit).map((hit) => hit.doc);
  };
}
