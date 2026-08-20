// The dark-mode toggle. Nothing else on the page needs script: the colours are
// CSS custom properties, and the OS preference already drives them until a
// reader states one of their own here.
//
// Reading the stored choice is `boot` in `parts.typ`, which runs before the
// first paint; this module only writes it.

const KEY = "albatros-theme";
const root = document.documentElement;

const isDark = () =>
  root.dataset.theme
    ? root.dataset.theme === "dark"
    : matchMedia("(prefers-color-scheme: dark)").matches;

for (const button of document.querySelectorAll("[data-theme-toggle]")) {
  const sync = () => button.setAttribute("aria-pressed", String(isDark()));
  sync();
  button.addEventListener("click", () => {
    root.dataset.theme = isDark() ? "light" : "dark";
    localStorage.setItem(KEY, root.dataset.theme);
    sync();
  });
}

/* Copy a code block ---------------------------------------------------------
   Added here rather than in the layout because the blocks are in the page's own
   body, which the template never sees. Absent without `navigator.clipboard`,
   which is what a page served over plain HTTP has: a button that cannot copy is
   worse than none. */

if (navigator.clipboard) {
  // The words, from the site's string table: the shell puts them on `<main>`,
  // which is the nearest element a template can reach (typst-html owns `<html>`).
  const strings = document.getElementById("main")?.dataset ?? {};
  const label = strings.copy ?? "Copy";
  const done = strings.copied ?? "Copied";

  for (const block of document.querySelectorAll("pre")) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "copy";
    button.textContent = label;
    button.addEventListener("click", async () => {
      await navigator.clipboard.writeText(block.querySelector("code")?.innerText ?? block.innerText);
      button.textContent = done;
      button.classList.add("copied");
      setTimeout(() => {
        button.textContent = label;
        button.classList.remove("copied");
      }, 1600);
    });
    // A wrapper, so the button can sit in the block's corner without the
    // absolute positioning scrolling away with the code.
    const wrap = document.createElement("div");
    wrap.className = "codeblock";
    block.replaceWith(wrap);
    wrap.append(block, button);
  }
}
