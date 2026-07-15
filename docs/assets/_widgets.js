// Small, self-contained widgets that dogfood baudelaire's `baudelaire:*` data
// modules: the build inlines the data at bundle time, so there is no runtime
// fetch and nothing to keep in sync by hand. Each mount no-ops when its target
// element is absent, so they cost nothing on pages that don't use them.

import feed from "baudelaire:feed";
import config from "baudelaire:config";

// Homepage "Latest writing": the most recent dated pages, straight from the
// same data that generates the RSS/Atom feeds.
export function mountRecent(limit = 5) {
  const mount = document.querySelector("[data-recent]");
  if (!mount || !feed.length) return;

  const fmt = new Intl.DateTimeFormat(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });

  const list = document.createElement("ul");
  list.className = "recent-list";
  for (const post of feed.slice(0, limit)) {
    const item = document.createElement("li");
    const link = document.createElement("a");
    link.href = post.url;
    link.className = "recent-link";

    const title = document.createElement("span");
    title.className = "recent-title";
    title.textContent = post.title;
    link.append(title);

    if (post.date) {
      const time = document.createElement("time");
      time.className = "recent-date";
      time.dateTime = post.date;
      time.textContent = fmt.format(new Date(post.date));
      link.append(time);
    }

    item.append(link);
    list.append(item);
  }
  mount.replaceChildren(list);
}

// Config page: print this site's live `client { }` block, exactly as any bundle
// would receive it from `baudelaire:config`.
export function mountConfig() {
  const mount = document.querySelector("[data-config]");
  if (!mount) return;
  const json = JSON.stringify(config, null, 2);
  const code = document.createElement("code");
  code.textContent = Object.keys(config).length
    ? json
    : "// the client { } block is empty";
  mount.replaceChildren(code);
}
