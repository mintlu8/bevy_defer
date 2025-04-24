use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use proc_macro_crate::{crate_name, FoundCrate};
use proc_macro_error::{abort, proc_macro_error};
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, parse_quote, spanned::Spanned, token::RArrow, DeriveInput, FnArg,
    GenericParam, Ident, ItemImpl, Pat, ReturnType, TraitItemFn, Type,
};

fn import_crate() -> TokenStream {
    let found_crate = crate_name("bevy_defer").expect("bevy_defer is not present in `Cargo.toml`");

    match found_crate {
        FoundCrate::Itself => quote!(crate),
        FoundCrate::Name(name) => {
            let ident = format_ident!("{}", name);
            quote!( #ident )
        }
    }
}

fn type_to_ident(ty: &Type) -> Ident {
    match ty {
        Type::Path(type_path) => match type_path.path.get_ident() {
            Some(ident) => ident.clone(),
            None => abort!(ty.span(), "Expected a single ident."),
        },
        _ => abort!(ty.span(), "Expected a single ident."),
    }
}

/// Mirror an `impl` block to an async access component.
///
/// ## Requirements
///
/// * For type `MyResource`, there must be a type `AsyncMyResource` with
///   an accessible field `0` being `AsyncResource<MyResource>`. This is the
///   semantics of the derive macros. Same for components and others.
///
/// * All functions must have `&self` or `&mut self` receivers.
///
/// * Outputs must be `'static`.
///
/// * Does not support `async` functions, since it's currently difficult to get a static future
///   with a `self` receiver. Return `impl Future + 'static` instead.
///
/// ```
/// use module::{Character, AsyncCharacter};
/// #[async_access]
/// impl Character {
///     fn get_name(&self) -> String {
///         ..
///     }
///     fn shoot(&mut self, angle: f32) {
///         ..
///     }
/// }
/// ```
///
/// ## Arguments
///
/// * `#[async_access(must_exist)]`
///
///     This will unwrap the results instead of returning `AccessResult`.
///     Useful on resources that should always be available.
#[proc_macro_attribute]
pub fn async_access(args: TokenStream1, tokens: TokenStream1) -> TokenStream1 {
    let unwraps = match syn::parse::<Ident>(args) {
        Ok(ident) => ident == format_ident!("must_exist"),
        Err(_) => false,
    };
    let original: TokenStream = tokens.clone().into();
    let impl_block = parse_macro_input!(tokens as ItemImpl);
    let bevy_defer = import_crate();
    let ty = type_to_ident(&impl_block.self_ty);
    let async_ty = format_ident!("Async{ty}");

    let (impl_generics, ty_generics, where_clause) = &impl_block.generics.split_for_impl();

    let mut functions = Vec::new();

    for item in &impl_block.items {
        let mut item_fn = match item {
            syn::ImplItem::Fn(f) => f.clone(),
            _ => abort!(item.span(), "Expected function."),
        };

        item_fn.sig.constness = None;

        let attrs = &item_fn.attrs;
        let vis = &item_fn.vis;
        let name = &item_fn.sig.ident;
        let is_mut = match item_fn.sig.inputs.first_mut() {
            Some(FnArg::Receiver(receiver)) => {
                if receiver.reference.is_none() {
                    abort!(receiver.span(), "Expected &self or &mut self.");
                }
                let result = receiver.mutability.is_some();
                *receiver = parse_quote!(&self);
                result
            }
            _ => abort!(item_fn.sig.inputs.span(), "Expected &self or &mut self."),
        };
        let method = if is_mut {
            quote! {get_mut}
        } else {
            quote! {get}
        };
        let unwrap_method = if unwraps {
            quote! {.unwrap()}
        } else {
            match item_fn.sig.output {
                ReturnType::Default => {
                    item_fn.sig.output =
                        ReturnType::Type(RArrow::default(), parse_quote!(#bevy_defer::AccessResult))
                }
                ReturnType::Type(arrow, ty) => {
                    item_fn.sig.output =
                        ReturnType::Type(arrow, parse_quote!(#bevy_defer::AccessResult<#ty>))
                }
            }
            quote! {}
        };
        let Ok(mut args) = item_fn
            .sig
            .inputs
            .iter()
            .skip(1)
            .map(|x| match x {
                FnArg::Receiver(_) => Err(()),
                FnArg::Typed(pat) => Ok(pat.pat.clone()),
            })
            .collect::<Result<Vec<_>, _>>()
        else {
            abort!(item_fn.sig.inputs.span(), "Error parsing arguments.");
        };

        for pat in &mut args {
            de_mutify(pat)
        }

        let sig = &item_fn.sig;
        functions.push(quote! {
            #(#attrs)*
            #vis #sig {
                use #bevy_defer::AsyncAccess;
                self.0.#method(|v| v.#name(#(#args),*)) #unwrap_method
            }
        })
    }

    quote! {
        #original
        #[allow(unused_mut)]
        const _: () = {
            impl #impl_generics #async_ty #ty_generics #where_clause {
                #(#functions)*
            }
        };
    }
    .into()
}

/// Generate type `Async{TypeName}` as a `AsyncComponentDeref` implementation.
#[proc_macro_derive(AsyncComponent)]
pub fn async_component(tokens: TokenStream1) -> TokenStream1 {
    async_access_deref(
        tokens.into(),
        format_ident!("AsyncComponent"),
        format_ident!("AsyncComponentDeref"),
    )
    .into()
}

/// Generate type `Async{TypeName}` as a `AsyncResourceDeref` implementation.
#[proc_macro_derive(AsyncResource)]
pub fn async_resource(tokens: TokenStream1) -> TokenStream1 {
    async_access_deref(
        tokens.into(),
        format_ident!("AsyncResource"),
        format_ident!("AsyncResourceDeref"),
    )
    .into()
}

/// Generate type `Async{TypeName}` as a `AsyncNonSendDeref` implementation.
#[proc_macro_derive(AsyncNonSend)]
pub fn async_non_send(tokens: TokenStream1) -> TokenStream1 {
    async_access_deref(
        tokens.into(),
        format_ident!("AsyncNonSend"),
        format_ident!("AsyncNonSendDeref"),
    )
    .into()
}

fn async_access_deref(tokens: TokenStream, ty: Ident, ty_deref: Ident) -> TokenStream {
    let Ok(input) = syn::parse2::<DeriveInput>(tokens.clone()) else {
        return tokens;
    };
    let generics = &input.generics;
    let (impl_generics, ty_generics, where_clause) = &input.generics.split_for_impl();
    let vis = input.vis;
    let name = input.ident;
    let bevy_defer = import_crate();
    let async_name = format_ident!("Async{name}");
    quote! {
        #[derive(Debug, #bevy_defer::RefCast)]
        #[repr(transparent)]
        #vis struct #async_name #generics (pub #bevy_defer::access::#ty<#name #ty_generics>);

        impl #impl_generics #bevy_defer::access::deref::#ty_deref for #name #ty_generics #where_clause{
            type Target = #async_name #ty_generics;

            fn async_deref(this: &#bevy_defer::access::#ty<Self>) -> &Self::Target {
                use #bevy_defer::RefCast;
                #async_name::ref_cast(this)
            }
        }
    }
}

fn de_mutify(pat: &mut Pat) {
    match pat {
        Pat::Ident(ident) => ident.mutability = None,
        Pat::Slice(slice) => {
            for elem in slice.elems.iter_mut() {
                de_mutify(elem)
            }
        }
        Pat::Struct(s) => {
            for elem in s.fields.iter_mut() {
                de_mutify(elem.pat.as_mut());
            }
        }
        Pat::Tuple(t) => {
            for elem in t.elems.iter_mut() {
                de_mutify(elem);
            }
        }
        Pat::TupleStruct(t) => {
            for elem in t.elems.iter_mut() {
                de_mutify(elem);
            }
        }
        // Might be incomplete? Make an issue if needed.
        _ => (),
    }
}

/// Turn an `async` function into a dyn compatible `Pin<Box<dyn Future>>`.
///
/// This is similar to `async-trait` but more tailored to `bevy_defer`'s game development needs.
///
/// # Key Differences
///
/// * Annotate functions instead of traits.
/// * Resulting future is by default `?Send`.
///
/// # Static-ness
///
/// In order for a resulting future to be static, the `self` receiver must be
/// `self: Rc<Self>` or `self: Arc<Self>`. Additionally no other lifetime
/// must be captured.
#[proc_macro_error]
#[proc_macro_attribute]
pub fn async_dyn(_: TokenStream1, tokens: TokenStream1) -> TokenStream1 {
    let mut func = parse_macro_input!(tokens as TraitItemFn);

    let mut capture_life_time = false;

    if func.sig.asyncness.is_none() {
        abort!(func.sig.fn_token.span, "Expected async function.");
    }
    func.sig.asyncness = None;

    for item in &mut func.sig.inputs {
        match item {
            FnArg::Receiver(receiver) => {
                if receiver.reference.is_some() {
                    capture_life_time = true;
                    break;
                }
            }
            FnArg::Typed(pat_type) => {
                if let Type::Reference(_) = pat_type.ty.as_mut() {
                    capture_life_time = true;
                    break;
                }
            }
        }
    }

    for param in &func.sig.generics.params {
        if let GenericParam::Lifetime(_) = param {
            capture_life_time = true;
            break;
        }
    }

    let lt = if capture_life_time {
        quote! {+ '_}
    } else {
        quote! {}
    };

    match func.sig.output {
        ReturnType::Default => {
            func.sig.output = parse_quote!(
                -> ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<
                    Output = ()
                > #lt>>
            );
            if let Some(body) = func.default {
                func.default = Some(parse_quote!({
                    ::std::boxed::Box::pin(async move {#body})
                }))
            }
        }
        ReturnType::Type(_, out) => {
            func.sig.output = parse_quote!(
                -> ::core::pin::Pin<::std::boxed::Box<dyn ::core::future::Future<
                    Output = #out
                > #lt>>
            );
            if let Some(body) = func.default {
                func.default = Some(parse_quote!({
                    ::std::boxed::Box::pin(async move {
                        let _out: #out = #body;
                        #[allow(unreachable_code)]
                        _out
                    })
                }))
            }
        }
    }

    quote! {#func}.into()
}
