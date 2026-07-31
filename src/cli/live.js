// The live-reload client, injected into every served HTML page.
//
// Two events arrive on one stream: the default one means the rebuild succeeded
// and the page should reload, and `failed` carries a rendered diagnostic. The
// second exists because a failed rebuild used to be invisible here: the browser
// kept showing the last good page, and nothing said whether the rebuild was
// slow, missed, or broken. You had to go back to the terminal to find out.
//
// Named `failed` rather than `error`, because `EventSource` dispatches its own
// transport errors to `error` listeners and the two would be indistinguishable.
//
// The same client carries the source-mapped preview: alt-click anything the
// build stamped with a `data-typst` location and the server opens it in the
// configured editor. Nothing happens on a page built without `html { spans }`,
// which carries no stamps to find.
(endpoint, open) => {
  const ID = "__baudelaire-overlay";

  const clear = () => document.getElementById(ID)?.remove();

  const show = (text) => {
    clear();
    const overlay = document.createElement("div");
    overlay.id = ID;
    overlay.setAttribute(
      "style",
      "position:fixed;inset:0;z-index:2147483647;overflow:auto;margin:0;" +
        "padding:2rem;background:rgba(20,18,24,.96);color:#f5f0ea;" +
        "font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace",
    );
    const title = document.createElement("div");
    title.setAttribute("style", "color:#ff8080;font-weight:700;margin-bottom:1rem");
    title.textContent = "rebuild failed · still serving the previous build";
    const body = document.createElement("pre");
    // textContent, never innerHTML: the diagnostic quotes source, and source is
    // not markup we control.
    body.textContent = text;
    body.setAttribute("style", "margin:0;white-space:pre-wrap;word-break:break-word");
    overlay.append(title, body);
    // Dismissable, so a stale overlay never traps the page it covers.
    overlay.addEventListener("click", clear);
    document.body.append(overlay);
  };

  // Alt-click, because it is the one modifier no element already claims: a
  // plain click follows links, and ctrl/cmd/shift open them elsewhere. The
  // nearest stamped ancestor wins, so clicking a word inside a paragraph asks
  // for the paragraph when the word itself was not an element of its own.
  document.addEventListener("click", (event) => {
    if (!event.altKey) return;
    const target = event.target.closest?.("[data-typst]");
    if (!target) return;
    // The click was for the editor: a link must not also navigate, and a
    // selection must not be left behind.
    event.preventDefault();
    const at = encodeURIComponent(target.getAttribute("data-typst"));
    fetch(`${open}?at=${at}`)
      // The server answers a refusal in plain text (no editor configured, a
      // file outside the project), which belongs on screen where the click was.
      .then((response) => (response.ok ? null : response.text().then(show)))
      .catch((error) => show(`could not reach the dev server: ${error}`));
  });

  const stream = new EventSource(endpoint);
  // A rebuild that succeeded: the reload replaces the whole document, overlay
  // included, so there is nothing to clear first.
  stream.onmessage = () => location.reload();
  stream.addEventListener("failed", (event) => show(JSON.parse(event.data)));
}
