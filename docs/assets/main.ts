// Entry point, bundled by rolldown from the partials below (files whose names
// start with `_` are never emitted on their own). Loaded as a module, so it
// runs after the DOM is parsed. Written in TypeScript, served as `main.js`:
// imports are extensionless, and `tsconfig.json` says what they resolve as.

import { initTheme } from "./_theme";
import {
  initMobileNav,
  initNavSections,
  keepNavScroll,
  markActiveNav,
} from "./_nav";
// The search palette is baudelaire's own generated client, pulled in as a
// virtual module so rolldown inlines and minifies it with the rest of the
// bundle. `styles: false` opts out of its default sheet; its `.bd-*` classes
// are themed by style.css instead.
import { mountSearch } from "baudelaire:search";
import { mountConfig, mountRecent } from "./_widgets";
import { initCopyButtons } from "./_copy";
import { initDemo, initEmit } from "./_home";

initTheme();
markActiveNav();
initNavSections();
// After the two above: it needs the active link marked and the sections it
// might sit inside already open.
keepNavScroll();
initMobileNav();
mountSearch({ styles: false, placeholder: "Search the docs" });
mountRecent();
mountConfig();
initCopyButtons();
// Landing page only; both no-op elsewhere.
initDemo();
initEmit();
