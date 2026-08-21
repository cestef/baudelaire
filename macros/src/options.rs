//! The `#[table(..)]` container attribute: what impl to write, what the table
//! is called, what expands a row, and anything else to put in the impl.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Ident, ImplItem, Path, Token, Type, braced, parenthesized};

/// A named directive the row macro expands in item position, for the associated
/// items that are derived from a field rather than written out.
pub(crate) struct Hook {
    name: Ident,
    args: TokenStream,
}

impl Hook {
    /// This hook as the row macro sees it: `rule!(@name Self, args);`.
    fn expand(&self, rule: &Path) -> TokenStream {
        let Self { name, args } = self;
        quote! { #rule!(@#name Self, #args); }
    }
}

impl Parse for Hook {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![=]>()?;
        Ok(Self {
            name,
            args: input.parse()?,
        })
    }
}

/// The associated const the rows are collected into: its name, its type, and
/// the constructor wrapping the slice, where there is one.
struct Held {
    name: Ident,
    ty: Type,
    ctor: Option<Path>,
}

impl Held {
    /// The const, with `rows` as its slice.
    fn expand(&self, rows: &[TokenStream]) -> TokenStream {
        let Self { name, ty, ctor } = self;
        let slice = quote! { &[#(#rows),*] };
        let value = ctor
            .as_ref()
            .map_or_else(|| slice.clone(), |ctor| quote! { #ctor(#slice) });
        quote! { const #name: #ty = #value; }
    }
}

impl Parse for Held {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<Token![const]>()?;
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let ty = input.parse()?;
        let ctor = match input.parse::<Option<Token![=]>>()? {
            Some(_) => Some(input.parse()?),
            None => None,
        };
        Ok(Self { name, ty, ctor })
    }
}

/// One setting written inside `#[table(..)]`.
enum Setting {
    Trait(Path),
    Held(Box<Held>),
    Rule(Path),
    Hook(Hook),
    Items(Vec<ImplItem>),
}

impl Setting {
    const UNKNOWN: &'static str = "unknown table setting: expected `impl = ..`, `const NAME: Type = Ctor`, `rule = ..`, `hook(name = ..)` or `items { .. }`";
}

impl Parse for Setting {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.peek(Token![impl]) {
            input.parse::<Token![impl]>()?;
            input.parse::<Token![=]>()?;
            return Ok(Self::Trait(input.parse()?));
        }
        if input.peek(Token![const]) {
            return Ok(Self::Held(Box::new(input.parse()?)));
        }
        let word: Ident = input.parse()?;
        match word.to_string().as_str() {
            "rule" => {
                input.parse::<Token![=]>()?;
                Ok(Self::Rule(input.parse()?))
            }
            "hook" => {
                let inner;
                parenthesized!(inner in input);
                Ok(Self::Hook(inner.parse()?))
            }
            "items" => {
                let inner;
                braced!(inner in input);
                let mut items = Vec::new();
                while !inner.is_empty() {
                    items.push(inner.parse()?);
                }
                Ok(Self::Items(items))
            }
            _ => Err(syn::Error::new(word.span(), Self::UNKNOWN)),
        }
    }
}

/// What `#[table(..)]` said, defaults filled in.
pub(crate) struct Options {
    tr: Path,
    held: Held,
    rule: Path,
    hooks: Vec<Hook>,
    items: Vec<ImplItem>,
}

impl Options {
    /// The trait implemented when nothing names one.
    const TRAIT: &'static str = "Section";
    /// The macro each row is expanded by when nothing names one.
    const RULE: &'static str = "rule";
    /// The const, its type and its constructor, when nothing names them.
    const HELD: &'static str = "const RULES: Block<Self> = Block";

    /// Reads every `#[table(..)]` on the item, later settings winning over
    /// earlier ones so a repeated key is an override rather than an error.
    pub(crate) fn read(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut options = Self::default()?;
        for attr in attrs.iter().filter(|a| a.path().is_ident("table")) {
            let settings =
                attr.parse_args_with(Punctuated::<Setting, Token![,]>::parse_terminated)?;
            for setting in settings {
                options.set(setting);
            }
        }
        Ok(options)
    }

    fn default() -> syn::Result<Self> {
        Ok(Self {
            tr: syn::parse_str(Self::TRAIT)?,
            held: syn::parse_str(Self::HELD)?,
            rule: syn::parse_str(Self::RULE)?,
            hooks: Vec::new(),
            items: Vec::new(),
        })
    }

    fn set(&mut self, setting: Setting) {
        match setting {
            Setting::Trait(path) => self.tr = path,
            Setting::Held(held) => self.held = *held,
            Setting::Rule(path) => self.rule = path,
            Setting::Hook(hook) => self.hooks.push(hook),
            Setting::Items(items) => self.items.extend(items),
        }
    }

    pub(crate) fn rule(&self) -> &Path {
        &self.rule
    }

    pub(crate) fn implements(&self) -> &Path {
        &self.tr
    }

    /// Everything inside the impl block: the verbatim items, the hooks, and the
    /// table itself.
    pub(crate) fn body(&self, rows: &[TokenStream]) -> TokenStream {
        let items = &self.items;
        let hooks = self.hooks.iter().map(|hook| hook.expand(&self.rule));
        let held = self.held.expand(rows);
        quote! {
            #(#items)*
            #(#hooks)*
            #held
        }
    }
}
