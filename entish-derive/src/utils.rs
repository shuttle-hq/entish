use std::collections::HashSet;

use proc_macro2::Ident;
use syn::{Variant, PathSegment, fold::{fold_ident, fold_variant, fold_path_segment, fold_type, Fold}};

#[derive(Debug, Default)]
pub struct MentionedGenerics {
    pub targets: HashSet<Ident>,
    mentioned: HashSet<Ident>
}

impl MentionedGenerics {
    pub fn new<'a, I: IntoIterator<Item = &'a Ident>>(targets: I) -> Self {
        Self {
            targets: targets.into_iter().cloned().collect(),
            mentioned: HashSet::new()
        }
    }
    pub fn into_mentioned(self) -> HashSet<Ident> {
        self.mentioned
    }
}

impl Fold for MentionedGenerics {
    fn fold_path_segment(&mut self, ps: PathSegment) -> PathSegment {
        if self.targets.contains(&ps.ident) {
            self.mentioned.insert(ps.ident.clone());
        }

        fold_path_segment(self, ps)
    }
}

#[derive(Debug)]
pub struct ReplaceIdent {
    replace: Ident,
    with: Ident
}

impl ReplaceIdent {
    pub fn replace_with(replace: Ident, with: Ident) -> Self {
        Self { replace, with }
    }
}

impl Fold for ReplaceIdent {
    fn fold_ident(&mut self, mut ident: Ident) -> Ident {
        if ident == self.replace {
            ident = self.with.clone();
        }

        fold_ident(self, ident)
    }
}

pub struct FindIdent {
    ident: Ident,
    matched: bool
}

impl Fold for FindIdent {
    fn fold_ident(&mut self, ident: Ident) -> Ident {
        if ident == self.ident {
            self.matched = true;
        }
        fold_ident(self, ident)
    }
}

impl FindIdent {
    pub fn new(ident: Ident) -> Self {
        Self {
            ident,
            matched: false
        }
    }

    pub fn matched(&self) -> bool {
        self.matched
    }
}

use syn::{punctuated::Punctuated, token::Comma, Fields, Field, FieldsNamed, FieldsUnnamed, Index};
use proc_macro2::TokenStream;

pub(crate) fn map_fields<F>(fields: &Fields, f: F) -> TokenStream
where
    F: Fn(&TokenStream, &Field) -> TokenStream
{
    match fields {
        Fields::Named(FieldsNamed { named, .. }) => {
            let mapped: Punctuated<TokenStream, Comma> = named.iter()
                .map(|field| {
                    let ident = {
                        let ident = &field.ident;
                        quote! { #ident }
                    };
                    let out = f(&ident, field);
                    quote! { #ident: #out }
                })
                .collect();
            quote! { {#mapped} }
        },
        Fields::Unnamed(FieldsUnnamed { unnamed, .. }) => {
            let mapped: Punctuated<TokenStream, Comma> = unnamed.iter()
                .enumerate()
                .map(|(idx, field)| {
                    let field_index: Index = idx.into();
                    let ident = quote! { #field_index };
                    f(&ident, field)
                })

                .collect();
            quote! { (#mapped) }
        },
        Fields::Unit => quote! {}
    }
}
