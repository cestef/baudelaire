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
(endpoint) => {
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

  const stream = new EventSource(endpoint);
  // A rebuild that succeeded: the reload replaces the whole document, overlay
  // included, so there is nothing to clear first.
  stream.onmessage = () => location.reload();
  stream.addEventListener("failed", (event) => show(JSON.parse(event.data)));
}
