// The two things every piece of the injected client needs: a way to build an
// element, and a way to put a stylesheet behind it.
//
// Elements are built, never written as markup. The client shares a document
// with the site it was injected into, and a diagnostic quotes source, which is
// not markup we control: `innerHTML` anywhere here would be one page away from
// rewriting the page it is reporting on.
() => {
  const el = (tag, attrs = {}, ...children) => {
    const node = document.createElement(tag);
    for (const [name, value] of Object.entries(attrs)) node.setAttribute(name, value);
    node.append(...children);
    return node;
  };

  // One stylesheet per component, keyed by id so re-injecting replaces it
  // rather than stacking a second copy.
  const style = (id, css) => {
    document.getElementById(id)?.remove();
    document.head.append(el("style", { id }, css));
  };

  return { el, style };
}
