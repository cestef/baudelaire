// The dark-mode toggle. The only script this theme ships: the language
// switcher is plain links, and the colours are custom properties the OS
// preference already drives until a reader states one of their own.

const KEY = "voyage-theme";
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
