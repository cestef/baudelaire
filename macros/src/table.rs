//! The derived impl: a struct's `#[key]` fields, in declaration order, under
//! the settings its `#[table]` attribute gave.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Generics, Ident};

use crate::key::Key;
use crate::options::Options;

pub(crate) struct Table {
    ident: Ident,
    generics: Generics,
    options: Options,
    keys: Vec<Key>,
}

impl Table {
    const SHAPE: &'static str = "`Table` derives from a struct with named fields";

    /// The fields of a named-field struct, in the order they are written.
    fn fields(data: &Data) -> syn::Result<&Fields> {
        match data {
            Data::Struct(item) if matches!(item.fields, Fields::Named(_)) => Ok(&item.fields),
            _ => Err(syn::Error::new(proc_macro2::Span::call_site(), Self::SHAPE)),
        }
    }

    pub(crate) fn expand(&self) -> TokenStream {
        let Self {
            ident,
            generics,
            options,
            keys,
        } = self;
        let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
        let rows: Vec<TokenStream> = keys.iter().map(|key| key.row(options.rule())).collect();
        let body = options.body(&rows);
        let tr = options.implements();
        quote! {
            #[automatically_derived]
            impl #impl_generics #tr for #ident #ty_generics #where_clause {
                #body
            }
        }
    }
}

impl TryFrom<DeriveInput> for Table {
    type Error = syn::Error;

    fn try_from(input: DeriveInput) -> syn::Result<Self> {
        let keys = Self::fields(&input.data)?
            .iter()
            .filter_map(|field| Key::of(field).transpose())
            .collect::<syn::Result<Vec<_>>>()?;
        Ok(Self {
            options: Options::read(&input.attrs)?,
            ident: input.ident,
            generics: input.generics,
            keys,
        })
    }
}
