// Copy-to-clipboard buttons on every code block. Each `<pre>` is wrapped in a
// positioned container so the button sits outside the block's horizontal scroll
// area; clicking copies the block's plain text (the highlight spans' content).

const COPY_ICON =
  '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h8"/></svg>';
const CHECK_ICON =
  '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>';

// Mount a button on one code block, wrapping its `<pre>` for positioning.
function mount(pre: HTMLPreElement): void {
  if (pre.parentElement?.classList.contains("code-block")) return;

  const wrap = document.createElement("div");
  wrap.className = "code-block";
  pre.replaceWith(wrap);
  wrap.append(pre);

  const button = document.createElement("button");
  button.type = "button";
  button.className = "copy-btn";
  button.setAttribute("aria-label", "Copy code");
  button.innerHTML = COPY_ICON;
  wrap.append(button);

  let revert: ReturnType<typeof setTimeout> | undefined;
  button.addEventListener("click", async () => {
    try {
      // `innerText`, not `textContent`: a block whose lines can be toggled (the
      // home page's config explorer) must copy what is on screen, not the lines
      // hidden behind an unticked box.
      await navigator.clipboard.writeText(pre.innerText);
    } catch {
      return; // no clipboard access (insecure context, denied permission)
    }
    button.classList.add("copied");
    button.innerHTML = CHECK_ICON;
    button.setAttribute("aria-label", "Copied");
    clearTimeout(revert);
    revert = setTimeout(() => {
      button.classList.remove("copied");
      button.innerHTML = COPY_ICON;
      button.setAttribute("aria-label", "Copy code");
    }, 1600);
  });
}

// Add a copy button to every code block in the article. No-ops on pages with
// none. The Clipboard API needs a secure context, so buttons quietly do nothing
// over plain HTTP; `baudelaire serve` and the deployed site are both fine.
export function initCopyButtons(): void {
  for (const pre of document.querySelectorAll<HTMLPreElement>("#main pre")) {
    mount(pre);
  }
}
