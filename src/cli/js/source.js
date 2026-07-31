// The source-mapped preview: alt-click anything the build stamped with a
// `data-typst` location and the dev server opens it in the configured editor.
//
// A page built without `html { spans }` carries no stamps, so the alt-click
// finds nothing and does nothing. The other way in is a diagnostic: the panel
// makes the `file:line:column` it names clickable, and says so by dispatching
// `baudelaire:open`, which is what the listener at the bottom answers.
(endpoint, overlay) => {
  const open = (at) => {
    const refused = (text) => overlay.show(text, "could not open the source", at);
    fetch(`${endpoint}?at=${encodeURIComponent(at)}`)
      // The server answers a refusal in plain text (no editor configured, a
      // file outside the project), which belongs on screen where the click was.
      .then((response) => (response.ok ? null : response.text().then(refused)))
      .catch((error) => refused(`could not reach the dev server: ${error}`));
  };

  // Alt-click, because it is the one modifier no element already claims: a
  // plain click follows links, and ctrl/cmd/shift open them elsewhere. The
  // nearest stamped ancestor wins, so clicking a word inside a paragraph asks
  // for the paragraph when the word itself was not an element of its own.
  document.addEventListener("click", (event) => {
    if (!event.altKey) return;
    const target = event.target.closest?.("[data-typst]");
    if (!target) return;
    // The click was for the editor: a link must not also navigate.
    event.preventDefault();
    open(target.getAttribute("data-typst"));
  });

  document.addEventListener("baudelaire:open", (event) => open(event.detail));
}
