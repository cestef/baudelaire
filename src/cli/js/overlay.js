// The panel the dev server puts a message on screen in: a failed rebuild's
// diagnostic, or a refusal from the editor endpoint. The message itself is laid
// out by `report`, which is the only thing here that knows what a diagnostic
// looks like.
//
// It brings no palette. System colours and one border, so it reads on any page
// and in either theme without a design of its own to keep in step with the
// site's, and without the page's stylesheet having a say in it.
//
// It closes on Escape, on the backdrop, and on its own button, but never on a
// click inside: the text is there to be selected and copied, and closing on any
// click threw the selection away every time.
(dom, report) => {
  const ID = "__baudelaire-overlay";

  dom.style(
    `${ID}-style`,
    `#${ID} { position: fixed; inset: 0; z-index: 2147483647; display: flex;
    align-items: center; justify-content: center; padding: 6vh 4vw;
    background: rgb(0 0 0 / 40%); color-scheme: light dark; }
  #${ID} > div { display: flex; flex-direction: column; width: min(96ch, 100%);
    max-height: 80vh; overflow: hidden; text-align: left; border-radius: 6px;
    background: light-dark(#fdfdfd, #191919); color: light-dark(#1b1b1b, #e8e8e8);
    border: 1px solid light-dark(#0000001f, #ffffff1f);
    font: 13px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; }
  #${ID} header { display: flex; align-items: baseline; gap: 1em;
    padding: .7em 1em; border-bottom: 1px solid light-dark(#00000014, #ffffff14); }
  #${ID} span { flex: 1; opacity: .55; overflow: hidden; text-overflow: ellipsis;
    white-space: nowrap; }
  #${ID} button { font: inherit; color: inherit; background: none; opacity: .7;
    border: 1px solid light-dark(#00000024, #ffffff24); border-radius: 4px;
    padding: 0 .5em; cursor: pointer; }
  #${ID} button:hover { opacity: 1; }
  #${ID} > div > div:last-child { padding: 1em; overflow: auto;
    user-select: text; }`,
  );

  // What the panel on screen listens to, aborted as one when it goes: an
  // overlay per failed rebuild used to leave its key handler behind.
  let listeners = null;

  const clear = () => {
    listeners?.abort();
    listeners = null;
    document.getElementById(ID)?.remove();
  };

  const show = (text, title, note = "") => {
    clear();
    listeners = new AbortController();
    const { signal } = listeners;

    const close = dom.el("button", { type: "button", "aria-label": "dismiss" }, "esc");
    close.addEventListener("click", clear, { signal });

    const overlay = dom.el(
      "div",
      { id: ID, role: "dialog" },
      dom.el(
        "div",
        {},
        dom.el("header", {}, dom.el("strong", {}, title), dom.el("span", {}, note), close),
        dom.el("div", {}, report(text)),
      ),
    );
    overlay.addEventListener(
      "click",
      (event) => {
        if (event.target === overlay) clear();
      },
      { signal },
    );
    document.addEventListener(
      "keydown",
      (event) => {
        if (event.key === "Escape") clear();
      },
      { signal },
    );
    document.body.append(overlay);
  };

  return { show, clear };
}
