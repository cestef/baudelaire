//! The `#[key(..)]` field attribute: one row of the table, and the doc comment
//! it is documented by.

use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned as _;
use syn::{Attribute, Expr, Field, Ident, Lit, LitStr, Meta, Path, Token};

/// A field's doc comment, reduced to the one line a table documents a key by.
struct Doc;

impl Doc {
    /// The first paragraph of `attrs`' doc comment, as one line.
    ///
    /// Only the first: a field may carry as much rustdoc as it likes below a
    /// blank line without any of it reaching the generated table.
    fn read(attrs: &[Attribute]) -> String {
        let mut lines = Vec::new();
        for line in attrs.iter().filter_map(Self::line) {
            let line = line.trim().to_owned();
            if line.is_empty() && !lines.is_empty() {
                break;
            }
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines.join(" ")
    }

    /// The text of one `#[doc = ".."]`, or `None` for any other attribute.
    fn line(attr: &Attribute) -> Option<String> {
        let Meta::NameValue(pair) = &attr.meta else {
            return None;
        };
        if !pair.path.is_ident("doc") {
            return None;
        }
        match &pair.value {
            Expr::Lit(lit) => match &lit.lit {
                Lit::Str(text) => Some(text.value()),
                _ => None,
            },
            _ => None,
        }
    }
}

/// The parsed body of a `#[key(..)]`: an optional name override, then whatever
/// the row macro reads.
struct Written {
    name: Option<Expr>,
    spec: TokenStream,
}

impl Parse for Written {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = Self::renamed(input)?;
        Ok(Self {
            name,
            spec: input.parse()?,
        })
    }
}

impl Written {
    /// A leading `name = ..`, which is a rename only when spelled exactly so;
    /// anything else is the start of the spec and is left where it is.
    ///
    /// Any expression, not only a literal: a key whose name is a const keeps
    /// naming itself through that const.
    fn renamed(input: ParseStream) -> syn::Result<Option<Expr>> {
        let ahead = input.fork();
        let Ok(word) = ahead.parse::<Ident>() else {
            return Ok(None);
        };
        if word != "name" || !ahead.peek(Token![=]) {
            return Ok(None);
        }
        input.parse::<Ident>()?;
        input.parse::<Token![=]>()?;
        let name = input.parse()?;
        input.parse::<Option<Token![,]>>()?;
        Ok(Some(name))
    }
}

/// One `#[key]` field, and everything the row macro is handed about it.
pub(crate) struct Key {
    name: Expr,
    field: Ident,
    doc: LitStr,
    spec: TokenStream,
}

impl Key {
    const UNNAMED: &'static str = "a `#[key]` field must have a name";

    /// The key a field is written under when nothing renames it: its own name,
    /// as a string literal.
    fn named(ident: &Ident) -> Expr {
        Expr::Lit(syn::ExprLit {
            attrs: Vec::new(),
            lit: Lit::Str(LitStr::new(&ident.to_string(), ident.span())),
        })
    }

    /// The key a field declares, or `None` for a field carrying no `#[key]` and
    /// so belonging to no table.
    pub(crate) fn of(field: &Field) -> syn::Result<Option<Self>> {
        let Some(attr) = field.attrs.iter().find(|a| a.path().is_ident("key")) else {
            return Ok(None);
        };
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new(field.span(), Self::UNNAMED))?;
        let written = match &attr.meta {
            Meta::Path(_) => Written {
                name: None,
                spec: TokenStream::new(),
            },
            _ => attr.parse_args_with(Written::parse)?,
        };
        let name = written.name.unwrap_or_else(|| Self::named(&ident));
        Ok(Some(Self {
            name,
            doc: LitStr::new(&Doc::read(&field.attrs), ident.span()),
            field: ident,
            spec: written.spec,
        }))
    }

    /// This key as the row macro sees it.
    pub(crate) fn row(&self, rule: &Path) -> TokenStream {
        let Self {
            name,
            field,
            doc,
            spec,
        } = self;
        quote! { #rule!(@row Self, #name, #field, #doc, #spec) }
    }
}
