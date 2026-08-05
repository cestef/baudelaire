// The version picker is a native `<details>`, so it opens and navigates with no
// JavaScript at all. What the element does not do is close when you decide
// against it: clicking elsewhere, tabbing away, or pressing Escape all leave it
// hanging open over the page.
//
// Only the picker is treated this way. The sidebar's `<details>` sections are
// disclosure widgets a reader opens deliberately and expects to stay open; a
// menu is the opposite, and closing on the next click is what makes it one.

export function initVersionPicker(): void {
  const picker = document.querySelector<HTMLDetailsElement>(".version-picker");
  if (!picker) return;

  const close = () => {
    picker.open = false;
  };

  // `pointerdown` rather than `click`: the menu should be gone by the time the
  // press resolves, and a press that starts outside still counts as leaving
  // even if it ends up as a drag or a text selection.
  document.addEventListener("pointerdown", (event) => {
    if (picker.open && !picker.contains(event.target as Node)) close();
  });

  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !picker.open) return;
    close();
    // Focus goes back to what opened it, or it would be left on an element
    // that is now hidden and the next Tab would start from the top of the page.
    picker.querySelector("summary")?.focus();
  });

  // Tabbing out closes it too. `relatedTarget` is null when focus leaves the
  // document entirely (another tab, the address bar), which is not a decision
  // about the menu, so that case is left alone.
  picker.addEventListener("focusout", (event) => {
    const moved = (event as FocusEvent).relatedTarget as Node | null;
    if (moved && !picker.contains(moved)) close();
  });
}
