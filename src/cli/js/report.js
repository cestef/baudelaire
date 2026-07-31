// A diagnostic, laid out as elements.
//
// The dev server sends exactly what it prints in the terminal: a code, a
// headline, and a source frame drawn with box characters. That is the right
// shape for a terminal and the wrong one for a page, so the frame is read back
// off and rebuilt here: the code as a tag, the headline as a heading, the
// snippet as numbered lines.
//
// Tolerant by construction. A line matching none of the shapes below is kept
// exactly as it arrived, so an unfamiliar report degrades to its own text
// rather than to nothing.
(dom) => {
  const ID = "__baudelaire-report";

  // The pieces miette draws, in the order they are tried.
  const NUMBERED = /^\s*(\d+)\s*│\s?(.*)$/;
  const CARET = /^\s*·\s?(.*)$/;
  const WHERE = /^\s*╭─+\[(.+)\]\s*$/;
  // The rules that only close a frame. Narrower than "starts with box art", so
  // that the location line above and a `╰─▶` cause below keep their text.
  const RULE = /^\s*[╭╰]─+\s*$/;
  const CAUSE = /^\s*╰─▶\s*(.*)$/;
  const HEADLINE = /^\s*([×⚠])\s+(.*)$/;
  const HELP = /^\s*help:\s*(.*)$/;
  const CODE = /^(?:Error:\s*)?([a-z][\w-]*(?:::[\w-]+)+)\s*$/;

  dom.style(
    `${ID}-style`,
    `.${ID} { display: flex; flex-direction: column; gap: .7em; }
  .${ID} p { margin: 0; }
  .${ID} .code { opacity: .45; }
  .${ID} .headline { font-weight: 700; }
  .${ID} .headline.error { color: light-dark(#a3312a, #f0a9a4); }
  .${ID} .caret { opacity: .5; }
  .${ID} .where { align-self: start; padding: 0; font: inherit; color: inherit;
    background: none; border: 0; opacity: .55; cursor: pointer;
    text-decoration: underline dotted; }
  .${ID} .where:hover { opacity: 1; }
  .${ID} .help { opacity: .8; }
  .${ID} .snippet { display: grid; grid-template-columns: auto 1fr; gap: 0 1em;
    padding: .5em .7em; overflow-x: auto; border-radius: 4px;
    border: 1px solid light-dark(#00000014, #ffffff14); }
  .${ID} .snippet span { opacity: .4; text-align: right; user-select: none; }
  .${ID} .snippet code { font: inherit; white-space: pre; }`,
  );

  return (text) => {
    const report = dom.el("div", { class: ID });
    // The frame being filled, or null between frames: a numbered line and the
    // carets under it belong to the same grid, anything else ends it.
    let snippet = null;
    const grid = () => {
      if (!snippet) report.append((snippet = dom.el("div", { class: "snippet" })));
      return snippet;
    };
    const row = (gutter, code, kind) =>
      grid().append(dom.el("span", {}, gutter), dom.el("code", { class: kind }, code));

    // The `file:line:column` a frame is labelled with is exactly what the
    // editor endpoint takes, so it is a button rather than a caption. It says
    // so by event, not by calling: this file knows how a diagnostic looks and
    // nothing else, and the piece that talks to the server is listening.
    const location = (at) => {
      const button = dom.el(
        "button",
        { type: "button", class: "where", title: "open in your editor" },
        at,
      );
      button.addEventListener("click", () =>
        document.dispatchEvent(new CustomEvent("baudelaire:open", { detail: at })),
      );
      return button;
    };

    for (const raw of text.split("\n")) {
      const line = raw.trimEnd();
      const numbered = NUMBERED.exec(line);
      if (numbered) {
        row(numbered[1], numbered[2], "line");
        continue;
      }
      const caret = CARET.exec(line);
      if (caret) {
        row("", caret[1], "caret");
        continue;
      }
      snippet = null;
      if (!line.trim() || RULE.test(line)) continue;
      const where = WHERE.exec(line);
      if (where) {
        report.append(location(where[1]));
        continue;
      }
      const matched = [
        [HEADLINE, "headline"],
        [HELP, "help"],
        [CAUSE, "text"],
        [CODE, "code"],
      ].find(([pattern]) => pattern.test(line));
      if (!matched) {
        report.append(dom.el("p", { class: "text" }, line));
        continue;
      }
      const [pattern, kind] = matched;
      const parts = pattern.exec(line);
      const [text, extra] =
        kind === "headline"
          ? [parts[2], parts[1] === "×" ? " error" : " warning"]
          : [parts[1], ""];
      report.append(
        dom.el("p", { class: kind + extra }, kind === "help" ? `help: ${text}` : text),
      );
    }
    return report;
  };
}
