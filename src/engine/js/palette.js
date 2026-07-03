// A self-mounting command palette over a baudelaire search index. It builds its
// own modal and, unless told not to, injects a deliberately minimal stylesheet:
// enough to be usable, nothing branded, everything expressed through `.bd-*`
// classes and custom properties so a host site restyles or replaces it wholesale
// (`mountSearch({ styles: false })` turns the defaults off entirely).
//
// `createSearch` and `tokenize` come from the engine this file is concatenated
// with (a single module scope, so they are in scope here). Exposes `mountSearch`,
// which the generated standalone client auto-calls and which bundlers import from
// the `baudelaire:search` virtual module.

const escapeHtml = (s) =>
  s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
const escapeRe = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

// The first URL segment, as a readable section label.
const sectionOf = (url) => {
  const seg = (url || "").split("/").filter(Boolean)[0];
  return seg ? seg[0].toUpperCase() + seg.slice(1) : "Home";
};

// Wrap every query term in <mark>, over HTML-escaped text.
const highlight = (text, terms) => {
  const safe = escapeHtml(text);
  return terms.length
    ? safe.replace(new RegExp("(" + terms.map(escapeRe).join("|") + ")", "gi"), "<mark>$1</mark>")
    : safe;
};

// A ~180-char window of the body around the first matched term.
const snippetOf = (body, terms) => {
  if (!body) return "";
  const lower = body.toLowerCase();
  let at = -1;
  for (const t of terms) {
    const i = lower.indexOf(t);
    if (i >= 0 && (at < 0 || i < at)) at = i;
  }
  if (at < 0) return body.slice(0, 150);
  const [start, end] = [Math.max(0, at - 60), Math.min(body.length, at + 120)];
  return (start ? "…" : "") + body.slice(start, end) + (end < body.length ? "…" : "");
};

// One hit as a list item. Its section, title, and (when present) snippet are the
// only structure; all appearance is left to `.bd-*` rules.
const rowOf = (doc, i, terms) => {
  const snippet = snippetOf(doc.body || "", terms);
  return `<li class="bd-hit" role="option" id="bd-hit-${i}" aria-selected="${i === 0}">
    <a href="${doc.url}" tabindex="-1">
      <span class="bd-section">${escapeHtml(sectionOf(doc.url))}</span>
      <span class="bd-title">${highlight(doc.title || doc.url, terms)}</span>
      ${snippet ? `<span class="bd-snippet">${highlight(snippet, terms)}</span>` : ""}
    </a></li>`;
};

const MARKUP = `
<div class="bd-scrim" data-close></div>
<div class="bd-panel" role="dialog" aria-modal="true" aria-label="Search">
  <input class="bd-input" type="search" role="combobox" aria-expanded="true"
    aria-controls="bd-list" aria-autocomplete="list" aria-label="Search"
    autocomplete="off" spellcheck="false" placeholder="Search">
  <ul class="bd-list" id="bd-list" role="listbox" aria-label="Search results"></ul>
  <p class="bd-empty">Start typing to search.</p>
</div>`;

// Minimal, neutral defaults — a centred overlay, a scrollable list, an active
// row. System colours, low specificity, no shadows or brand hues; a host is
// expected to layer its own look on the same classes.
const DEFAULT_STYLES = `
.bd-palette{position:fixed;inset:0;z-index:2147483000;display:flex;
  justify-content:center;align-items:flex-start;padding:12vh 1rem 1rem;color-scheme:light dark}
.bd-palette[hidden]{display:none}
.bd-scrim{position:absolute;inset:0;background:rgba(0,0,0,.4)}
.bd-panel{position:relative;width:100%;max-width:34rem;max-height:70vh;display:flex;
  flex-direction:column;background:Canvas;color:CanvasText;border:1px solid;border-radius:.5rem;overflow:hidden}
.bd-input{padding:.75rem 1rem;border:0;border-bottom:1px solid;outline:0;background:transparent;color:inherit;font:inherit}
.bd-list{margin:0;padding:.25rem;list-style:none;overflow-y:auto}
.bd-list[hidden],.bd-empty[hidden]{display:none}
.bd-hit>a{display:block;padding:.4rem .6rem;border-radius:.35rem;color:inherit;text-decoration:none}
.bd-hit.is-active>a{background:Highlight;color:HighlightText}
.bd-section{font-size:.75rem;opacity:.7}
.bd-title{display:block;font-weight:600}
.bd-snippet{display:block;font-size:.8rem;opacity:.7;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.bd-empty{margin:0;padding:1.25rem;text-align:center;opacity:.7}`;

// The palette instance: owns its DOM, its lazily-loaded searcher, and the
// interaction state. Constructed by `mountSearch`.
class Palette {
  constructor({ url, limit = 12, placeholder, styles = true } = {}) {
    this.url = url;
    this.limit = limit;
    this.searcher = null; // resolved search(query) function
    this.ready = null; // pending createSearch()
    this.opener = null; // element to refocus on close
    this.hits = [];
    this.active = -1;

    if (styles && !document.getElementById("bd-search-style")) {
      const style = document.createElement("style");
      style.id = "bd-search-style";
      style.textContent = DEFAULT_STYLES;
      document.head.append(style);
    }

    this.root = document.createElement("div");
    this.root.className = "bd-palette";
    this.root.hidden = true;
    this.root.innerHTML = MARKUP;
    document.body.append(this.root);

    this.input = this.root.querySelector(".bd-input");
    this.list = this.root.querySelector(".bd-list");
    this.empty = this.root.querySelector(".bd-empty");
    if (placeholder) this.input.placeholder = placeholder;

    this.input.addEventListener("input", () => this.render());
    this.input.addEventListener("keydown", (e) => this.onKey(e));
    this.root.querySelector("[data-close]").addEventListener("click", () => this.close());
    this.list.addEventListener("mousemove", (e) => this.onHover(e));
    this.list.addEventListener("click", (e) => e.target.closest("a") && this.close());
  }

  async open(opener) {
    this.opener = opener || null;
    this.root.hidden = false;
    document.body.style.overflow = "hidden";
    this.input.focus();
    this.input.select();
    if (!this.searcher) {
      this.ready ??= createSearch(this.url);
      this.searcher = await this.ready;
    }
    this.render();
  }

  close() {
    this.root.hidden = true;
    document.body.style.overflow = "";
    this.opener?.focus();
  }

  render() {
    const query = this.input.value.trim();
    this.hits = query && this.searcher ? this.searcher(query, { limit: this.limit }) : [];
    this.active = this.hits.length ? 0 : -1;

    const empty = !query || !this.hits.length;
    this.list.hidden = empty;
    this.empty.hidden = !empty;
    if (empty) {
      this.list.innerHTML = "";
      this.empty.textContent = query ? `No results for “${query}”.` : "Start typing to search.";
      this.input.removeAttribute("aria-activedescendant");
      return;
    }

    const terms = tokenize(query);
    this.list.innerHTML = this.hits.map((doc, i) => rowOf(doc, i, terms)).join("");
    this.select(0);
  }

  // Mark row `i` active, moving the ARIA + visual selection and scrolling it in.
  select(i) {
    this.active = i;
    [...this.list.children].forEach((li, n) => {
      const on = n === i;
      li.setAttribute("aria-selected", String(on));
      li.classList.toggle("is-active", on);
      if (on) {
        li.scrollIntoView({ block: "nearest" });
        this.input.setAttribute("aria-activedescendant", li.id);
      }
    });
  }

  onKey(e) {
    const keys = {
      ArrowDown: () => this.hits.length && this.select((this.active + 1) % this.hits.length),
      ArrowUp: () => this.hits.length && this.select((this.active - 1 + this.hits.length) % this.hits.length),
      Enter: () => this.list.children[this.active]?.querySelector("a")?.click(),
      Escape: () => this.close(),
    };
    if (keys[e.key]) {
      e.preventDefault();
      keys[e.key]();
    }
  }

  onHover(e) {
    const li = e.target.closest("[role=option]");
    const i = li && [...this.list.children].indexOf(li);
    if (li && i !== this.active) this.select(i);
  }

  // Wire existing `[data-search-open]` triggers and the global hotkey: Cmd/Ctrl+K
  // always, plus `key` (default "/") when not typing in a field.
  bind(key) {
    for (const btn of document.querySelectorAll("[data-search-open]")) {
      btn.addEventListener("click", () => this.open(btn));
    }
    addEventListener("keydown", (e) => {
      if (!this.root.hidden) return;
      const typing = /^(?:INPUT|TEXTAREA|SELECT)$/.test(e.target.tagName) || e.target.isContentEditable;
      if (((e.key === "k" || e.key === "K") && (e.metaKey || e.ctrlKey)) || (e.key === key && !typing)) {
        e.preventDefault();
        this.open(document.querySelector("[data-search-open]"));
      }
    });
    return this;
  }
}

// Mount the palette and return its controller. Idempotent: a repeat call returns
// the first instance, so an import and the auto-mount can coexist.
// Options: { url, limit, placeholder, hotkey, styles }.
export function mountSearch(options = {}) {
  if (typeof document === "undefined") return null;
  return (window.__baudelaireSearch ??= new Palette(options).bind(options.hotkey || "/"));
}
