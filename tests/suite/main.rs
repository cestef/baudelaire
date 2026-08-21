//! Every integration test, in one binary.
//!
//! One target rather than one per file because each links the whole crate:
//! twenty-eight of them was ten gigabytes of `target/` and most of what a
//! build costs. nextest runs each test in its own process, so the isolation
//! a separate binary used to provide is unchanged.

mod common;

#[cfg(feature = "announce")]
mod announce_e2e;
mod build_e2e;
#[cfg(feature = "cards")]
mod cards_e2e;
mod cli_e2e;
mod config_e2e;
mod deploy_e2e;
#[cfg(feature = "ssh")]
mod deploy_ssh_e2e;
mod discovery;
mod external_e2e;
mod features_e2e;
mod frontmatter;
mod history_e2e;
mod i18n_e2e;
mod images_e2e;
mod incremental_e2e;
mod navigation_e2e;
#[cfg(feature = "pdf")]
mod pdf_e2e;
mod reference;
mod scaffold_e2e;
mod scenarios;
mod serve_e2e;
mod sources_e2e;
mod static_e2e;
mod subpath_e2e;
mod svg_e2e;
mod theme_e2e;
mod themes_e2e;
mod typst_modules_e2e;
#[cfg(feature = "js")]
mod virtual_modules_e2e;
