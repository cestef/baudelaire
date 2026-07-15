// Entry point, bundled by rolldown from the partials below (files whose names
// start with `_` are never emitted on their own). Loaded as a module, so it
// runs after the DOM is parsed.

import { initTheme } from "./_theme.js";
import { initMobileNav, initNavSections, markActiveNav } from "./_nav.js";
// The search palette is baudelaire's own generated client, pulled in as a
// virtual module so rolldown inlines and minifies it with the rest of the
// bundle. `styles: false` opts out of its default sheet; its `.bd-*` classes
// are themed by style.css instead.
import { mountSearch } from "baudelaire:search";
import { mountConfig, mountRecent } from "./_widgets.js";

initTheme();
markActiveNav();
initNavSections();
initMobileNav();
mountSearch({ styles: false, placeholder: "Search the docs" });
mountRecent();
mountConfig();
