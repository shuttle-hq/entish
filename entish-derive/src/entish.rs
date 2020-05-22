use proc_macro2::{Ident, TokenStream};

use std::collections::HashSet;
use std::iter::FromIterator;

use syn::{fold::{Fold, fold_fields_named, fold_fields_unnamed}, Generics, Variant, Type, GenericParam, TypeParam, punctuated::Punctuated, token::Comma, Fields, FieldsNamed, FieldsUnnamed, Attribute, DeriveInput, Meta, MetaList, Path, NestedMeta, TypeParamBound, Index};

use crate::utils::{MentionedGenerics, ReplaceIdent, FindIdent, map_fields};

const SELF: &'static str = "Self";
const CHILD: &'static str = "Child";
const MAP_OUTPUT: &'static str = "MapOutput";

macro_rules! generic_param {
    ($e:expr) => {
        GenericParam::Type(
            TypeParam {
                attrs: Vec::new(),
                ident: $e,
                colon_token: None,
                bounds: Punctuated::new(),
                eq_token: None,
                default: None
            }
        )
    }
}

fn is_ident(ty: &Type, ident: &Ident) -> bool {
    ty == &syn::parse2(quote! { #ident }).unwrap()
}

fn contains_ident(ty: &Type, ident: &Ident) -> bool {
    let mut find_ident = FindIdent::new(ident.clone());
    find_ident.fold_type(ty.clone());
    find_ident.matched()
}

fn add_bound_to_all<'a, I>(iter: I, lt: TypeParamBound) -> ()
where
    I: Iterator<Item = &'a mut GenericParam>
{
    iter.for_each(|param| {
        match param {
            GenericParam::Type(tp) => {
                tp.bounds.push(lt.clone());
            },
            _ => {}
        };
    })
}

fn all_but_ident<'a, I>(iter: I, ident: Ident) -> impl Iterator<Item = &'a mut GenericParam>
where
    I: Iterator<Item = &'a mut GenericParam>
{
    iter.filter(move |gp| {
        match gp {
            GenericParam::Type(tp) => {
                if tp.ident == ident {
                    false
                } else {
                    true
                }
            },
            _ => true
        }
    })
}

fn where_clause_for_generics<'a, I>(iter: I) -> Option<TokenStream>
where
    I: Iterator<Item = &'a GenericParam>
{
    let where_predicates: Punctuated<TokenStream, Comma> = iter
        .filter_map(|gp| {
            match gp {
                GenericParam::Type(TypeParam { ident, .. }) => {
                    if ident != &format_ident!("{}", CHILD) {
                        Some(quote! { #ident: Clone })
                    } else {
                        None
                    }
                },
                _ => None
            }
        })
        .collect();

    if ! where_predicates.is_empty() {
        Some(quote! { where #where_predicates })
    } else {
        None
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
enum SupportedDerives {
    TryInto,
    From,
    Map,
    MapOwned,
    IntoResult,
    IntoOption
}

impl SupportedDerives {
    fn try_from(p: &Path) -> Option<Self> {
        match p.get_ident()?.to_string().as_str() {
            "TryInto" => Some(Self::TryInto),
            "From" => Some(Self::From),
            "Map" => Some(Self::Map),
            "MapOwned" => Some(Self::MapOwned),
            "IntoResult" => Some(Self::IntoResult),
            "IntoOption" => Some(Self::IntoOption),
            _ => None
        }
    }
}

#[derive(Debug)]
pub struct NodeBuilder {
    generics: Generics,
    variant: Variant,
    relevant: MentionedGenerics,
}

pub struct Node {
    ident: Ident,
    generics: Generics,
    fields: Fields
}

impl NodeBuilder {
    pub fn from_variant(generics: &Generics, variant: &Variant) -> Self {
        Self {
            generics: generics.clone(),
            variant: variant.clone(),
            relevant: MentionedGenerics::default()
        }
    }

    pub fn extend_closure<'a, I: IntoIterator<Item = &'a Ident>>(
        &mut self,
        i: I
    ) -> &mut Self {
        self.relevant.targets.extend(i.into_iter().cloned());
        self
    }

    pub fn generate(mut self) -> Node {
        let mut out = TokenStream::new();

        syn::fold::fold_variant(&mut self.relevant, self.variant.clone());
        let closure = self.relevant.into_mentioned();

        let ident = self.variant.ident;

        let generics: Generics = {
            let mut params = Punctuated::<GenericParam, Comma>::new();
            let params = self.generics
                .params
                .into_iter()
                .filter_map(|p| match p {
                    GenericParam::Type(TypeParam { ident, .. }) if closure.contains(&ident) => {
                        Some(generic_param!(ident))
                    },
                    _ => None
                })
                .collect();
            Generics {
                params,
                ..Default::default()
            }
        };

        Node { ident, generics, fields: self.variant.fields }
    }
}

#[derive(Debug)]
pub struct EntishBuilder {
    ident: Ident,
    attributes: Vec<Attribute>,
    generics: Generics,
    nodes: Vec<NodeBuilder>,
    variants_as_structs: bool
}

impl EntishBuilder {
    pub fn new(input: &DeriveInput) -> Self {

        let ident = input.ident.clone();

        let mut generics = input.generics.clone();
        generics.params.push(generic_param!(format_ident!("{}", CHILD)).into());

        let attributes = input.attrs.clone();

        Self {
            ident,
            attributes,
            generics,
            nodes: Vec::new(),
            variants_as_structs: true
        }
    }

    pub fn get_generic_idents(&self) -> impl Iterator<Item = &'_ Ident> + '_ {
        self.generics
            .params
            .iter()
            .filter_map(|gp| {
                match gp {
                    GenericParam::Type(TypeParam { ident, .. }) => {
                        Some(ident)
                    },
                    _ => None
                }
            })
    }

    pub fn add_node(&mut self, variant: &Variant) -> &mut Self {
        let mut ri = ReplaceIdent::replace_with(
            format_ident!("{}", SELF),
            format_ident!("{}", CHILD)
        );

        let variant = ri.fold_variant(variant.clone());
        let mut node = NodeBuilder::from_variant(&self.generics, &variant);

        node.extend_closure(self.get_generic_idents());

        self.nodes.push(node);
        self
    }

    pub fn generate(mut self) -> TokenStream {
        let mut out = TokenStream::new();

        let child_ident = format_ident!("{}", CHILD);

        let c_ident = self.ident;
        let c_generics = self.generics;

        let mut metas = Vec::new();
        let mut derives = HashSet::new();

        let trait_ident = format_ident!("{}Tree", c_ident);

        // sift through attributes; remove all paths in #[derive(..)]
        // if path is supported
        for attribute in self.attributes.into_iter() {
            let mut meta = attribute.parse_meta().unwrap();
            match &mut meta {
                Meta::List(MetaList { path, nested, .. }) => {
                    if path.get_ident() == Some(&format_ident!("derive")) {
                        *nested = nested
                            .iter()
                            .cloned()
                            .filter_map(|nested_meta| match nested_meta {
                                NestedMeta::Meta(Meta::Path(p)) => {
                                    match SupportedDerives::try_from(&p) {
                                        Some(derive) => {
                                            derives.insert(derive);
                                            None
                                        },
                                        None => Some(NestedMeta::Meta(Meta::Path(p)))
                                    }
                                },
                                _ => panic!("only paths allowed here")
                            })
                            .collect();
                    } else if path.get_ident() == Some(&format_ident!("entish")) {
                        continue
                    }
                },
                _ => {}
            }
            metas.push(meta);
        }

        let attributes: TokenStream = metas.into_iter()
            .map(|meta| quote! { #[#meta] })
            .collect();

        let mut variants = Punctuated::<Variant, Comma>::new();

        for node in self.nodes.into_iter() {
            // add variant to container enum
            // generate fully qualified variant
            let Node { ident, generics, fields } = node.generate();

            let fields_stream = match &fields {
                Fields::Named(n) => quote! { #n },
                Fields::Unnamed(un) => quote! { #un; },
                Fields::Unit => quote! { ; }
            };

            variants.push(syn::parse2(quote! { #ident(#ident#generics) }).unwrap());

            let ident_doc = format!(
                "A node of type `{ident}` in a [{trait_}](trait.{trait_}.html)",
                ident = ident,
                trait_ = trait_ident
            );
            out.extend(quote! {
                #[doc = #ident_doc]
                #attributes
                pub struct #ident#generics #fields_stream
            });

            if derives.contains(&SupportedDerives::From) {
                out.extend(quote! {
                    impl#c_generics From<#ident#generics> for #c_ident#c_generics {
                        fn from(variant: #ident#generics) -> Self {
                            Self::#ident(variant)
                        }
                    }
                });
            }

            if derives.contains(&SupportedDerives::TryInto) {
                out.extend(quote! {
                    impl#c_generics std::convert::TryInto<#ident#generics>
                        for #c_ident#c_generics
                    {
                        type Error = ();
                        fn try_into(self) ->
                            std::result::Result<#ident#generics, Self::Error>
                        {
                            match self {
                                Self::#ident(variant) => Ok(variant),
                                _ => Err(())
                            }
                        }
                    }
                });
            }

            if derives.contains(&SupportedDerives::Map) ||
                derives.contains(&SupportedDerives::MapOwned)
            {
                let map_output_ident = format_ident!("{}", MAP_OUTPUT);                

                let mapped_fields = map_fields(&fields, |ident, field| {
                    let ty = &field.ty;
                    if is_ident(ty, &child_ident) {
                        // assumes nesting gp
                        quote! { f(&self.#ident) }
                    } else if contains_ident(&field.ty, &child_ident) {
                        // assumes container type
                        quote! { <#ty as Map<&'a Child, MapOutput>>::map(&self.#ident, f) }
                    } else {
                        // assumes has to move
                        quote! { self.#ident.clone() }
                    }
                });

                let mapped_fields_owned = map_fields(&fields, |ident, field| {
                    let ty = &field.ty;
                    if is_ident(ty, &child_ident) {
                        // assumes nesting gp
                        quote! { f(self.#ident) }
                    } else if contains_ident(&field.ty, &child_ident) {
                        // assumes container type
                        quote! { <#ty as MapOwned<Child, MapOutput>>::map_owned(self.#ident, f) }
                    } else {
                        // assumes has to move
                        quote! { self.#ident.clone() }
                    }
                });

                let mut rg = ReplaceIdent::replace_with(
                    format_ident!("{}", CHILD),
                    map_output_ident.clone()
                );
                let mapped_generics = rg.fold_generics(generics.clone());

                let mut map_generic_params = generics.params.clone();
                add_bound_to_all(
                    all_but_ident(map_generic_params.iter_mut(), format_ident!("{}", CHILD)),
                    syn::parse2(quote! { 'a }).unwrap()
                );

               let mut map_owned_generic_params = generics.params.clone(); 

                let where_clause = where_clause_for_generics(generics.params.iter());

                let has_child = map_generic_params
                    .iter()
                    .any(|p| {
                        match p {
                            GenericParam::Type(TypeParam { ident, .. }) => {
                                ident == &child_ident
                            },
                            _ => false
                        }
                    });

                if ! has_child {
                    map_generic_params.push(syn::parse2(quote! { #child_ident }).unwrap());
                    map_owned_generic_params.push(syn::parse2(quote! { #child_ident }).unwrap());
                }

                if derives.contains(&SupportedDerives::Map) {
                    out.extend(quote! {
                        impl<'a, #map_output_ident: 'a, #map_generic_params>
                            entish::Map<'a, &'a #child_ident, #map_output_ident>
                            for #ident#generics
                            #where_clause
                        {
                            type OuterO = #ident#mapped_generics;
                            fn map<F>(&'a self, f: &mut F) -> Self::OuterO
                            where
                                F: FnMut(&'a #child_ident) -> #map_output_ident
                            {
                                #ident #mapped_fields
                            }
                        }
                    });
                }

                if derives.contains(&SupportedDerives::MapOwned) {
                    out.extend(quote! {
                        impl<#map_output_ident, #map_owned_generic_params>
                            entish::MapOwned<#child_ident, #map_output_ident>
                            for #ident#generics
                            #where_clause
                        {
                            type OuterO = #ident#mapped_generics;
                            fn map_owned<F>(self, f: &mut F) -> Self::OuterO
                            where
                                F: FnMut(#child_ident) -> #map_output_ident
                            {
                                #ident #mapped_fields_owned
                            }
                        }
                    });                    
                }
            }

            if derives.contains(&SupportedDerives::IntoResult) {
                let err_tp: TypeParam = syn::parse2(quote! { __Error }).unwrap();

                let mut generics_with_e = generics.clone();
                generics_with_e.params.push(GenericParam::Type(err_tp.clone()));

                let generic_args: Punctuated<TokenStream, Comma> = generics
                    .params
                    .iter()
                    .cloned()
                    .map(|param| {
                        if param == generic_param!(format_ident!("{}", CHILD)) {
                            quote! { std::result::Result<#param, #err_tp> }
                        } else {
                            quote! { #param }
                        }
                    })
                    .collect();

                let mapped_fields = map_fields(&fields, |ident, field| {
                    if is_ident(&field.ty, &child_ident) {
                        // assumes nesting gp
                        quote! { self.#ident? }
                    } else if contains_ident(&field.ty, &child_ident) {
                        // assumes container type
                        quote! { self.#ident.into_result()? }
                    } else {
                        // assumes has to move
                        quote! { self.#ident }
                    }
                });

                out.extend(quote! {
                    impl#generics_with_e
                        entish::IntoResult<#ident#generics, #err_tp>
                        for #ident<#generic_args>
                    {
                        fn into_result(self) -> std::result::Result<#ident#generics, #err_tp> {
                            Ok(
                                #ident #mapped_fields
                            )
                        }
                    }
                });
            }

            if derives.contains(&SupportedDerives::IntoOption) {
                let generic_args: Punctuated<TokenStream, Comma> = generics
                    .params
                    .iter()
                    .cloned()
                    .map(|param| {
                        if param == generic_param!(format_ident!("{}", CHILD)) {
                            quote! { Option<#param> }
                        } else {
                            quote! { #param }
                        }
                    })
                    .collect();

                let mapped_fields = map_fields(&fields, |ident, field| {
                    if is_ident(&field.ty, &child_ident) {
                        // assumes nesting gp
                        quote! { self.#ident? }
                    } else if contains_ident(&field.ty, &child_ident) {
                        // assumes container type
                        quote! { self.#ident.into_option()? }
                    } else {
                        // assumes has to move
                        quote! { self.#ident }
                    }
                });

                out.extend(quote! {
                    impl#generics
                        entish::IntoOption<#ident#generics>
                        for #ident<#generic_args>
                    {
                        fn into_option(self) -> Option<#ident#generics> {
                            Some(
                                #ident #mapped_fields
                            )
                        }
                    }
                });
            }
        }

        if derives.contains(&SupportedDerives::Map) ||
            derives.contains(&SupportedDerives::MapOwned)
        {
            let map_output_ident = format_ident!("{}", MAP_OUTPUT);

            let map_variants: Punctuated<TokenStream, Comma> = variants.iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    quote! { Self::#ident(variant) => variant.map(f).into() }
                })
                .collect();

            let map_variants_owned: Punctuated<TokenStream, Comma> = variants.iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    quote! { Self::#ident(variant) => variant.map_owned(f).into() }
                })
                .collect();

            let mut rg = ReplaceIdent::replace_with(
                format_ident!("{}", CHILD),
                map_output_ident.clone()
            );

            let mapped_c_generics = rg.fold_generics(c_generics.clone());

            let mut map_generic_params = c_generics.params.clone();
            add_bound_to_all(
                all_but_ident(map_generic_params.iter_mut(), format_ident!("{}", CHILD)),
                syn::parse2(quote! { 'a }).unwrap()
            );

            let map_owned_generic_params = c_generics.params.clone();

            let c_where_clause = where_clause_for_generics(c_generics.params.iter());

            if derives.contains(&SupportedDerives::Map) {
                out.extend(quote! {
                    impl<'a, #map_output_ident: 'a, #map_generic_params>
                        entish::Map<'a, &'a #child_ident, #map_output_ident>
                        for #c_ident#c_generics
                        #c_where_clause
                    {
                        type OuterO = #c_ident#mapped_c_generics;
                        fn map<F>(&'a self, f: &mut F) -> Self::OuterO
                        where
                            F: FnMut(&'a #child_ident) -> #map_output_ident
                        {
                            match self {
                                #map_variants
                            }
                        }
                    }
                });
            }

            if derives.contains(&SupportedDerives::MapOwned) {
                out.extend(quote! {
                    impl<#map_output_ident, #map_owned_generic_params>
                        entish::MapOwned<#child_ident, #map_output_ident>
                        for #c_ident#c_generics
                        #c_where_clause
                    {
                        type OuterO = #c_ident#mapped_c_generics;
                        fn map_owned<F>(self, f: &mut F) -> Self::OuterO
                        where
                            F: FnMut(#child_ident) -> #map_output_ident
                        {
                            match self {
                                #map_variants_owned
                            }
                        }
                    }
                });
            }
        }

        if derives.contains(&SupportedDerives::IntoOption) {
            let generic_args: Punctuated<TokenStream, Comma> = c_generics
                .params
                .iter()
                .cloned()
                .map(|param| {
                    if param == generic_param!(format_ident!("{}", CHILD)) {
                        quote! { Option<#param> }
                    } else {
                        quote! { #param }
                    }
                })
                .collect();

            let mapped_variants: Punctuated<TokenStream, Comma> = variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    quote! { Self::#ident(variant) => variant.into_option()?.into() }
                })
                .collect();

            out.extend(quote! {
                impl#c_generics
                    entish::IntoOption<#c_ident#c_generics>
                    for #c_ident<#generic_args>
                {
                    fn into_option(self) -> Option<#c_ident#c_generics> {
                        Some(
                            match self {
                                #mapped_variants
                            }
                        )
                    }
                }
            });
        }

        if derives.contains(&SupportedDerives::IntoResult) {
            let err_tp: TypeParam = syn::parse2(quote! { __Error }).unwrap();

            let mut c_generics_with_e = c_generics.clone();
            c_generics_with_e.params.push(GenericParam::Type(err_tp.clone()));

            let generic_args: Punctuated<TokenStream, Comma> = c_generics
                .params
                .iter()
                .cloned()
                .map(|param| {
                    if param == generic_param!(format_ident!("{}", CHILD)) {
                        quote! { std::result::Result<#param, #err_tp> }
                    } else {
                        quote! { #param }
                    }
                })
                .collect();

            let mapped_variants: Punctuated<TokenStream, Comma> = variants
                .iter()
                .map(|variant| {
                    let ident = &variant.ident;
                    quote! { Self::#ident(variant) => variant.into_result()?.into() }
                })
                .collect();

            out.extend(quote! {
                impl#c_generics_with_e
                    entish::IntoResult<#c_ident#c_generics, #err_tp>
                    for #c_ident<#generic_args>
                {
                    fn into_result(self) -> std::result::Result<#c_ident#c_generics, #err_tp> {
                        Ok(
                            match self {
                                #mapped_variants
                            }
                        )
                    }
                }
            });
        }

        let mut c_generics_no_child = c_generics.clone();
        c_generics_no_child.params.pop();

        let c_where_clause = where_clause_for_generics(c_generics_no_child.params.iter());

        let mut c_generics_with_o = c_generics_no_child.clone();
        c_generics_with_o.params.push(generic_param!(format_ident!("O")));

        let mut c_generics_with_ref_self: Punctuated<TokenStream, Comma> = c_generics_no_child
            .clone()
            .params
            .into_iter()
            .map(|c| quote! { #c })
            .collect();
        c_generics_with_ref_self.push(quote! { &Self });

        let mut c_generics_with_self = c_generics_no_child.clone();
        c_generics_with_self.params.push(generic_param!(format_ident!("Self")));

        let maybe_try_fold_impl = if derives.contains(&SupportedDerives::IntoResult) {
            Some(quote! {
                /// Like `try_fold` but when the operation can fail
                fn try_fold<F, O, E>(self, f: &mut F) -> std::result::Result<O, E>
                where
                    F: FnMut(#c_ident#c_generics_with_o) -> std::result::Result<O, E>
                {
                    let arg = self.into_inner()
                        .map_owned(&mut |c| c.try_fold(f))
                        .into_result()?;
                    f(arg)
                }
            })
        } else {
            None
        };

        let trait_doc = format!(
            "A trait for types that are like a tree whose nodes are described by [{c_ident}](enum.{c_ident}.html).",
            c_ident = c_ident
        );

        out.extend(quote! {
            #[doc = #trait_doc]
            pub trait #trait_ident#c_generics_no_child: Sized
                #c_where_clause
            {
                /// Unravel a node whose children are references to my
                /// children
                fn as_ref(&self) -> #c_ident<#c_generics_with_ref_self>;

                fn into_inner(self) -> #c_ident#c_generics_with_self;

                /// Reduce the tree to a single value using by folding a
                /// closure, recursively reducing from leaves to root
                fn fold<F, O>(self, f: &mut F) -> O
                where
                    F: FnMut(#c_ident#c_generics_with_o) -> O
                {
                    let arg = self.into_inner().map_owned(&mut |c| c.fold(f));
                    f(arg)
                }

                #maybe_try_fold_impl

                /// Get an iterator over references to children of this node
                fn iter_children<'a, I>(&'a self) -> std::vec::IntoIter<&'a Self>
                {
                    let mut children = Vec::new();
                    self.as_ref().map(&mut |&c| children.push(c));
                    children.into_iter()
                }
            }
        });

        let mut example_generics: Vec<String> = c_generics_no_child
            .params
            .into_iter()
            .filter_map(|gp| match gp {
                GenericParam::Type(TypeParam { ident, .. }) =>
                    Some(format!("{}", ident)),
                _ => None
            })
            .collect();
        example_generics.push(format!("Box<Self>"));
        let example_generics_ = example_generics.as_slice().join(", ");
        let c_ident_doc = format!(
            "A node in a tree [{trait_}](trait.{trait_}.html) whose children are of type `{child}`.

It can be made into an actual tree by replacing `Child` by a type implementing [{trait_}](trait.{trait_}.html). This could be done by simply adding some recursive dynamic indirection such as
```
pub struct My{trait_}({container}<{generics}>);
```
",
            trait_ = trait_ident,
            child = child_ident,
            container = c_ident,
            generics = example_generics_
        );
        out.extend(quote! {
            #[doc = #c_ident_doc]
            #attributes
            pub enum #c_ident#c_generics {
                #variants
            }
        });

        out
    }
}
