use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{format_ident, quote, ToTokens};
use syn::{parse_quote, token::RArrow, DeriveInput, FnArg, Ident, ItemImpl, ReturnType};

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

/// Mirror an `impl` block to an async access component.
///
/// ## Requirements
///
/// * For type `MyResource`, there must be a type `AsyncMyResource` with
/// an accessible field `0` being `AsyncResource<MyResource>`. This is the
/// semantics of the derive macros.
///
/// * All functions must have `&self` or `&mut self` receivers.
///
/// * Outputs must be `'static`.
/// 
/// * Does not support `async` functions, return `impl Future + 'static` instead.
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
///     This will unwrap the results instead of returning `AsyncResult`. 
///     Useful on resources that should always be available.
#[proc_macro_attribute]
pub fn async_access(args: TokenStream1, tokens: TokenStream1) -> TokenStream1 {
    async_access2(args.into(), tokens.into()).into()
}

fn async_access2(args: TokenStream, tokens: TokenStream) -> TokenStream {
    let unwraps = match syn::parse2::<Ident>(args) {
        Ok(ident) => ident == format_ident!("must_exist"),
        Err(_) => false,
    };
    let Ok(impl_block) = syn::parse2::<ItemImpl>(tokens.clone()) else {
        return quote! {#tokens compile_error!("Expected impl block.")};
    };

    let bevy_defer = import_crate();
    let ty = match syn::parse2::<Ident>(impl_block.self_ty.into_token_stream()) {
        Ok(type_name) => type_name,
        Err(_) => return quote! {#tokens compile_error!("Expected type name ident.")},
    };

    let async_ty = format_ident!("Async{ty}");

    let (impl_generics, ty_generics, where_clause) = &impl_block.generics.split_for_impl();

    let mut functions = Vec::new();

    macro_rules! parse_error {
        () => {
            return quote! {#tokens compile_error!("Only supports fn with &self or &mut self parameters.")}
        };
    }
    for item in &impl_block.items {
        let mut item_fn = match item {
            syn::ImplItem::Fn(f) => f.clone(),
            _ => parse_error!(),
        };

        let attrs = &item_fn.attrs;
        let vis = &item_fn.vis;
        let name = &item_fn.sig.ident;
        let is_mut = match item_fn.sig.inputs.first_mut() {
            Some(FnArg::Receiver(receiver)) => {
                if receiver.reference.is_none() {
                    parse_error!();
                }
                let result = receiver.mutability.is_some();
                *receiver = parse_quote!(&self);
                result
            }
            _ => parse_error!(),
        };
        let method = if is_mut {
            quote! {set}
        } else {
            quote! {get}
        };
        let unwrap_method = if unwraps {
            quote! {.unwrap()}
        } else {
            match item_fn.sig.output {
                ReturnType::Default => {
                    item_fn.sig.output =
                        ReturnType::Type(RArrow::default(), parse_quote!(#bevy_defer::AsyncResult))
                }
                ReturnType::Type(arrow, ty) => {
                    item_fn.sig.output =
                        ReturnType::Type(arrow, parse_quote!(#bevy_defer::AsyncResult<#ty>))
                }
            }
            quote! {}
        };
        let Ok(args) = item_fn
            .sig
            .inputs
            .iter()
            .skip(1)
            .map(|x| match x {
                FnArg::Receiver(_) => Err(()),
                FnArg::Typed(pat) => Ok(&pat.pat),
            })
            .collect::<Result<Vec<_>, _>>()
        else {
            parse_error!()
        };
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
        #tokens
        const _: () = {
            impl #impl_generics #async_ty #ty_generics #where_clause {
                #(#functions)*
            }
        };
    }
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
        #vis struct #async_name #generics (pub #bevy_defer::access::#ty<#name>);

        impl #impl_generics #bevy_defer::access::deref::#ty_deref for #name #ty_generics #where_clause{
            type Target = #async_name;

            fn async_deref(this: &#bevy_defer::access::#ty<Self>) -> &Self::Target {
                use #bevy_defer::RefCast;
                #async_name::ref_cast(this)
            }
        }
    }
}
