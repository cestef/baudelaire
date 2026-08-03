//! Virtual JS modules: the `baudelaire:*` specifiers a user's bundle can import
//! and have inlined, tree-shaken, minified, and fingerprinted like first-party
//! code. Each [`Module`] generates ES-module source from the site's build data;
//! [`Virtual`] is the single rolldown plugin that serves them all, assembled
//! from the registry in [`builtin`]. Adding a module is one impl and one line.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::future::Future;
use std::sync::Arc;

use rolldown::plugin::{
    HookLoadArgs, HookLoadOutput, HookLoadReturn, HookResolveIdArgs, HookResolveIdOutput,
    HookResolveIdReturn, HookUsage, Plugin, PluginContext, SharedLoadPluginContext,
};
use rolldown_common::ModuleType;

use crate::codegen::{Js, Value};
use crate::config::{Config, Named};
use crate::content::Page;
use crate::engine::emit::dts::Dts;
use crate::render::AssetMap;

mod client;
mod data;
mod declare;

pub use declare::Declarations;

/// The read-only build data every virtual module generates from.
pub(super) struct ModuleCx<'a> {
    pub config: &'a Config,
    pub pages: &'a [Page],
    pub assets: &'a AssetMap,
    /// The `sys.inputs.baudelaire` value: `baudelaire:site` / `:config` serve
    /// its sub-trees, so Typst and JS read one build context.
    pub context: &'a Value,
    /// The section tree value, already built for `page.sections`.
    pub sections: &'a Value,
}

/// One provider of `baudelaire:*` virtual modules.
pub(super) trait Module {
    /// The `specifier -> ES-module source` pairs this module serves. Most return
    /// one; [`Search`] serves a family (bare plus per-format).
    fn entries(&self, cx: &ModuleCx) -> Vec<(String, String)>;

    /// The `specifier -> declaration` pairs typing what [`entries`] serves.
    /// Nothing on disk holds these modules, so a TypeScript entry importing one
    /// reads it as unknown until [`Declarations`] writes these out.
    ///
    /// No default: a module that served JavaScript and declared nothing would
    /// be an import a typed bundle cannot use, and the gap would be silent.
    ///
    /// [`entries`]: Module::entries
    fn types(&self, cx: &ModuleCx) -> Vec<(String, Dts)>;
}

/// The registered virtual modules. Adding one is an impl plus a line here, so
/// the list is a `Vec` rather than a fixed-size array whose length is a second
/// place to edit.
fn builtin() -> Vec<Box<dyn Module>> {
    vec![
        Box::new(client::Search),
        Box::new(client::Navigation),
        Box::new(data::Site),
        Box::new(data::Settings),
        Box::new(data::Assets),
        Box::new(data::Pages),
        Box::new(data::Sections),
        Box::new(data::Taxonomies),
        Box::new(data::Feed),
        Box::new(data::I18n),
    ]
}

/// The one rolldown plugin serving every virtual module: a flat `specifier ->
/// source` table built once from [`builtin`] against the site context.
#[derive(Debug)]
pub(super) struct Virtual {
    modules: HashMap<String, Arc<str>>,
}

impl Virtual {
    pub(super) fn new(cx: &ModuleCx) -> Self {
        let modules = builtin()
            .iter()
            .flat_map(|module| module.entries(cx))
            .map(|(id, src)| (id, Arc::from(src.as_str())))
            .collect();
        Self { modules }
    }
}

impl Plugin for Virtual {
    fn name(&self) -> Cow<'static, str> {
        "baudelaire:virtual".into()
    }

    fn resolve_id(
        &self,
        _ctx: &PluginContext,
        args: &HookResolveIdArgs<'_>,
    ) -> impl Future<Output = HookResolveIdReturn> + Send {
        // Claim our specifiers so rolldown skips filesystem resolution.
        let resolved = self
            .modules
            .contains_key(args.specifier)
            .then(|| HookResolveIdOutput::from_id(args.specifier));
        async move { Ok(resolved) }
    }

    fn load(
        &self,
        _ctx: SharedLoadPluginContext,
        args: &HookLoadArgs<'_>,
    ) -> impl Future<Output = HookLoadReturn> + Send {
        let loaded = self.modules.get(args.id).map(|code| HookLoadOutput {
            code: code.to_string().into(),
            module_type: Some(ModuleType::Js),
            ..HookLoadOutput::default()
        });
        async move { Ok(loaded) }
    }

    fn register_hook_usage(&self) -> HookUsage {
        HookUsage::ResolveId | HookUsage::Load
    }
}

/// Displays a [`Named`] enum's spellings as a TypeScript string union, so a
/// declaration offers exactly the names the config parses.
pub(super) struct Names<T: Named + 'static>(&'static [(&'static str, T)]);

impl<T: Named> std::fmt::Display for Names<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (name, _)) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(" | ")?;
            }
            write!(f, "\"{name}\"")?;
        }
        Ok(())
    }
}

/// ES-module source builders, shared by the data modules so they emit exports
/// one way: a default export always, plus a named const per top-level object
/// key that is a safe identifier (so `import { title }` tree-shakes).
pub(super) struct Esm;

impl Esm {
    /// The declaration matching [`Esm::object`]: the same named consts and the
    /// same default, typed from the value itself, so a site's own keys are what
    /// an editor completes.
    fn typed(value: &Value) -> Dts {
        let mut dts = Dts::new();
        if let Value::Dict(pairs) = value {
            for (key, item) in pairs {
                if Self::ident(key) {
                    dts = dts.constant(key, item);
                }
            }
        }
        dts.default(value)
    }

    /// A module exporting `value` as default and each valid-identifier key of it
    /// (when it is a dict) as a named const.
    fn object(value: &Value) -> String {
        let mut out = String::new();
        if let Value::Dict(pairs) = value {
            for (key, item) in pairs {
                if Self::ident(key) {
                    let _ = writeln!(out, "export const {key} = {};", Js(item));
                }
            }
        }
        let _ = writeln!(out, "export default {};", Js(value));
        out
    }

    /// A module with a single default export of `value`.
    fn value(value: &Value) -> String {
        format!("export default {};\n", Js(value))
    }

    /// Whether `s` is a safe, non-reserved JS identifier for a named export.
    /// The spelling of an identifier is [`codegen::ident`]; a reserved word is
    /// a legal object key but cannot be bound, which is this module's concern
    /// and not the renderer's.
    ///
    /// [`codegen::ident`]: crate::codegen::ident
    fn ident(s: &str) -> bool {
        const RESERVED: &[&str] = &[
            "default", "class", "const", "let", "var", "function", "return", "import", "export",
            "new", "delete", "typeof", "in", "of", "do", "if", "else", "switch", "case", "for",
            "while", "break", "continue", "this", "super", "void", "yield", "await", "null",
            "true", "false",
        ];
        crate::codegen::ident(s) && !RESERVED.contains(&s)
    }
}
