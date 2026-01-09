//! Derive macros for moss.

use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derive the `Merge` trait for a struct.
///
/// Generates an implementation that calls `.merge()` on each field.
/// All fields must implement `Merge`.
///
/// # Example
///
/// ```ignore
/// use moss::Merge;
///
/// #[derive(Merge)]
/// struct Config {
///     enabled: bool,
///     name: Option<String>,
/// }
/// ```
#[proc_macro_derive(Merge)]
pub fn derive_merge(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let merge_impl = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => {
                let field_merges = fields.named.iter().map(|f| {
                    let field_name = &f.ident;
                    quote! {
                        #field_name: ::rhizome_moss_core::Merge::merge(self.#field_name, other.#field_name)
                    }
                });
                quote! {
                    Self {
                        #(#field_merges),*
                    }
                }
            }
            Fields::Unnamed(fields) => {
                let field_merges = (0..fields.unnamed.len()).map(|i| {
                    let index = syn::Index::from(i);
                    quote! {
                        ::rhizome_moss_core::Merge::merge(self.#index, other.#index)
                    }
                });
                quote! {
                    Self(#(#field_merges),*)
                }
            }
            Fields::Unit => quote! { Self },
        },
        Data::Enum(_) => {
            return syn::Error::new_spanned(&input, "Merge cannot be derived for enums")
                .to_compile_error()
                .into();
        }
        Data::Union(_) => {
            return syn::Error::new_spanned(&input, "Merge cannot be derived for unions")
                .to_compile_error()
                .into();
        }
    };

    let expanded = quote! {
        impl #impl_generics ::rhizome_moss_core::Merge for #name #ty_generics #where_clause {
            fn merge(self, other: Self) -> Self {
                #merge_impl
            }
        }
    };

    TokenStream::from(expanded)
}
