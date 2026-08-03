// Landing-page widgets. Both are progressive enhancement over markup the build
// already wrote: the tabbed example ships every variant rendered, and the
// generate-block explorer ships its config lines highlighted. Nothing here
// renders content the compiler could have rendered.

// Tabbed source/output example. Every pane is in the document; this only decides
// which one is shown, with the arrow-key behaviour a tablist owes a keyboard.
export function initDemo() {
  for (const demo of document.querySelectorAll("[data-demo]")) {
    const tablist = demo.querySelector("[data-demo-tabs]");
    if (!tablist) continue;
    const tabs = [...tablist.querySelectorAll("[data-demo-tab]")];
    const panes = [...demo.querySelectorAll("[data-demo-pane]")];
    if (tabs.length < 2) continue;
    tablist.hidden = false;

    const show = (index, focus) => {
      tabs.forEach((tab, i) => {
        const active = i === index;
        tab.classList.toggle("is-active", active);
        tab.setAttribute("aria-selected", String(active));
        tab.tabIndex = active ? 0 : -1;
        if (active && focus) tab.focus();
      });
      panes.forEach((pane, i) => {
        pane.hidden = i !== index;
      });
    };

    tablist.addEventListener("click", (event) => {
      const tab = event.target.closest("[data-demo-tab]");
      if (tab) show(tabs.indexOf(tab), false);
    });

    tablist.addEventListener("keydown", (event) => {
      const step = { ArrowRight: 1, ArrowLeft: -1, Home: 0, End: 0 }[event.key];
      if (step === undefined) return;
      event.preventDefault();
      const current = tabs.findIndex((tab) => tab.tabIndex === 0);
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? tabs.length - 1
            : (current + step + tabs.length) % tabs.length;
      show(next, true);
    });
  }
}

// Two 14px lucide glyphs, inlined the way `_copy.js` inlines its own: the tree
// is built here, so it cannot reach the `svg()` the templates use.
const FOLDER_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/></svg>';
const FILE_ICON =
  '<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/></svg>';

// One `<ul>` per directory level. A generated file (one a ticked option wrote)
// carries `is-new`, which is what the accent colour in the stylesheet keys on,
// so ticking a box lights up exactly the files that box is responsible for.
function branch(nodes) {
  const list = document.createElement("ul");
  list.className = "tree-list";
  for (const node of nodes) {
    const item = document.createElement("li");
    const row = document.createElement("span");
    row.className = `tree-row${node.children ? " is-dir" : ""}${node.generated ? " is-new" : ""}`;
    row.innerHTML = node.children ? FOLDER_ICON : FILE_ICON;
    const name = document.createElement("span");
    name.className = "tree-name";
    name.textContent = node.name;
    row.append(name);
    item.append(row);
    if (node.children) item.append(branch(node.children));
    list.append(item);
  }
  return list;
}

// The `generate { }` explorer: ticking a box reveals that option's config line
// and adds the files it writes to the output tree.
export function initEmit() {
  const root = document.querySelector("[data-emit]");
  if (!root) return;
  const switches = [...root.querySelectorAll("[data-emit-option]")];
  const treePane = root.querySelector("[data-emit-tree-pane]");
  const tree = root.querySelector("[data-emit-tree]");
  if (!switches.length || !tree) return;

  for (const control of switches) control.disabled = false;
  treePane.hidden = false;

  const on = (control) => control.getAttribute("aria-checked") === "true";
  const list = (control, key) =>
    (control.dataset[key] || "").split(",").filter(Boolean);

  const draw = () => {
    const checked = switches.filter(on);
    for (const control of switches) {
      const line = root.querySelector(
        `[data-emit-line="${control.dataset.emitOption}"]`,
      );
      if (line) line.hidden = !on(control);
    }

    // A page directory holds its index plus whatever per-page artifacts are on.
    const perPage = checked.flatMap((control) => list(control, "pageFiles"));
    const page = (name) => ({
      name,
      children: [
        { name: "index.html" },
        ...perPage.map((file) => ({ name: file, generated: true })),
      ],
    });
    const rootFiles = checked
      .flatMap((control) => list(control, "files"))
      .sort();

    tree.replaceChildren(
      branch([
        { name: "index.html" },
        { name: "assets/", children: [{ name: "style.b4f1a0.css" }] },
        { name: "blog/", children: [page("hello/")] },
        ...rootFiles.map((name) => ({ name, generated: true })),
      ]),
    );
  };

  // A `role="switch"` button owns its state, so flip it here; the browser
  // already gives it Enter and Space.
  root.addEventListener("click", (event) => {
    const control = event.target.closest("[data-emit-option]");
    if (!control) return;
    control.setAttribute("aria-checked", String(!on(control)));
    draw();
  });
  draw();
}
