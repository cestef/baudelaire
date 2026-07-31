// The dark-mode toggle. Nothing else on the page needs script: the layout is
// grid and the colours are custom properties, so the OS preference drives them
// until a reader states one of their own here.

const KEY = "paysage-theme";
const root = document.documentElement;

const stored = localStorage.getItem(KEY);
if (stored === "light" || stored === "dark") {
  root.dataset.theme = stored;
}

const current = () =>
  root.dataset.theme ??
  (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");

for (const button of document.querySelectorAll("[data-theme-toggle]")) {
  button.addEventListener("click", () => {
    const next = current() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    localStorage.setItem(KEY, next);
  });
}
