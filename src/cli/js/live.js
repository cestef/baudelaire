// The live-reload client: one event stream, and a dot in the corner saying
// whether it is still there.
//
// Two events arrive on the stream. The default one means the rebuild succeeded
// and the page should reload; `failed` carries a rendered diagnostic. The
// second exists because a failed rebuild used to be invisible here: the browser
// kept showing the last good page, and nothing said whether the rebuild was
// slow, missed, or broken. You had to go back to the terminal to find out.
//
// Named `failed` rather than `error`, because `EventSource` dispatches its own
// transport errors to `error` listeners and the two would be indistinguishable.
(endpoint, overlay, dom) => {
  const ID = "__baudelaire-status";

  // Discreet on purpose: it is a dot in the corner of somebody's page, and it
  // has nothing to say while everything works. It takes its colour from the
  // page until something is wrong.
  dom.style(
    `${ID}-style`,
    `#${ID} { position: fixed; right: .8rem; bottom: .8rem; z-index: 2147483646;
    width: .5rem; height: .5rem; padding: 0; border: 0; border-radius: 50%;
    background: currentColor; opacity: .2; cursor: pointer;
    transition: opacity .15s; }
  #${ID}:hover, #${ID}:focus-visible { opacity: .75; }
  #${ID}[data-state="lost"] { background: #d29922; opacity: .75; }
  #${ID}[data-state="failed"] { background: #f85149; opacity: .85; }`,
  );

  const dot = dom.el("button", {
    id: ID,
    type: "button",
    "aria-label": "live reload status",
  });
  document.body.append(dot);

  // What the dot stands for, and what clicking it shows: the diagnostic while
  // one is outstanding, the connection's state otherwise. A rebuild that
  // succeeds reloads the page, which is what clears a failure.
  let report = { state: "live", title: "live reload connected", note: "", text: "" };

  const set = (next) => {
    report = { note: "", text: "", ...next };
    dot.dataset.state = report.state;
    dot.title = report.title;
  };

  set(report);
  dot.addEventListener("click", () => overlay.show(report.text, report.title, report.note));

  const stream = new EventSource(endpoint);
  stream.onopen = () => set({ state: "live", title: "live reload connected" });
  // `EventSource` reconnects on its own, so this is a state and not a failure:
  // the dot says the page may be stale until the stream is back.
  stream.onerror = () =>
    set({
      state: "lost",
      title: "live reload disconnected",
      note: "reconnecting",
      text: "The dev server stopped answering. This page may be out of date; it reloads by itself once the server is back.",
    });
  // The reload replaces the whole document, dot and overlay included, so there
  // is nothing to clear first.
  stream.onmessage = () => location.reload();
  stream.addEventListener("failed", (event) => {
    const text = JSON.parse(event.data);
    set({
      state: "failed",
      title: "rebuild failed",
      note: "still serving the previous build",
      text,
    });
    overlay.show(text, report.title, report.note);
  });
}
