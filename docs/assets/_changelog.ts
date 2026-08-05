// The changelog page.
//
// The template marks each release with `data-release`, so nothing here has to
// infer the page's shape from heading levels. What this adds is the navigation
// a long changelog needs and HTML has no element for: a rail of every version,
// tracking whichever one you are reading.
//
// Progressive enhancement throughout. Without it the page is the whole
// changelog, correctly styled, and every release is still reachable by its own
// anchor.

interface Release {
  head: HTMLElement;
  /// Everything between this release's heading and the next one.
  body: HTMLElement[];
  /// `null` for `Unreleased`, which is newer than every version.
  version: number[] | null;
  label: string;
  id: string;
  link: HTMLAnchorElement;
}

/// `0.0.11` as `[0, 0, 11]`, or null when the heading names no version.
function parse(text: string): number[] | null {
  const match = /(\d+)\.(\d+)\.(\d+)/.exec(text);
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : null;
}

/// The `2026-08-05` a release heading ends with, lifted out so the rail and the
/// header can show it apart from the version.
function splitDate(head: HTMLElement): string | null {
  // `h3`, not `h2`: typst-html renders a level-2 heading one level down, so
  // the release heading a `##` produced is an `h3` in the document.
  const heading = head.querySelector("h3");
  if (!heading) return null;
  const text = heading.textContent || "";
  const match = /\s[-–]\s*(\d{4}-\d{2}-\d{2})\s*$/.exec(text);
  if (!match) return null;
  // Trim it off whichever text node actually ends the heading, so the link and
  // any other markup inside are left alone.
  for (const node of [...heading.childNodes].reverse()) {
    if (node.nodeType !== Node.TEXT_NODE) continue;
    const value = node.nodeValue || "";
    if (!value.includes(match[1])) continue;
    node.nodeValue = value.slice(0, value.lastIndexOf(match[0]));
    break;
  }
  return match[1];
}

function collect(article: Element): Release[] {
  const found: Release[] = [];
  for (const node of article.children) {
    const element = node as HTMLElement;
    if (element.hasAttribute("data-release")) {
      const label = (element.textContent || "").trim();
      const id = element.querySelector("[id]")?.id || "";
      found.push({
        head: element,
        body: [],
        version: parse(label),
        label,
        id,
        link: document.createElement("a"),
      });
    } else {
      found[found.length - 1]?.body.push(element);
    }
  }
  return found;
}

export function initChangelog(): void {
  // `article.changelog`, not `.changelog article`: `shell(class: ..)` puts the
  // class *on* the article, so the descendant form matches nothing and the
  // whole enhancement silently never ran.
  const article = document.querySelector("article.changelog");
  if (!article) return;
  const releases = collect(article);
  if (releases.length < 2) return;

  const nav = document.createElement("nav");
  nav.className = "changelog-nav";
  nav.setAttribute("aria-label", "Releases");
  const count = document.createElement("span");
  count.className = "changelog-count";

  for (const release of releases) {
    const date = splitDate(release.head);
    if (date) {
      const stamp = document.createElement("span");
      stamp.className = "release-date";
      stamp.textContent = date;
      release.head.append(stamp);
    }

    // The rail entry: the version alone, since the date is already beside the
    // release it belongs to.
    release.link.href = `#${release.id}`;
    release.link.textContent = release.version
      ? release.version.join(".")
      : "Unreleased";
    if (!release.version) release.link.dataset.unreleased = "true";
    nav.append(release.link);
  }

  count.textContent = `${releases.length} releases`;
  nav.append(count);
  releases[0].head.before(nav);

  // Reading the rail: whichever release is nearest the top of the viewport is
  // the one being read. `IntersectionObserver` rather than a scroll handler, so
  // this costs nothing while the page is still.
  const seen = new Set<Release>();
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        const release = releases.find((r) => r.head === entry.target);
        if (!release) continue;
        if (entry.isIntersecting) seen.add(release);
        else seen.delete(release);
      }
      const current = releases.find((r) => seen.has(r));
      for (const release of releases) {
        release.link.toggleAttribute("aria-current", release === current);
      }
    },
    // A band across the upper page: a release counts as "being read" while its
    // heading is anywhere near the top, not only in the instant it crosses.
    { rootMargin: "-15% 0px -70% 0px" },
  );
  for (const release of releases) observer.observe(release.head);

}
