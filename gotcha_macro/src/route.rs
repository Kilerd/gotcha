use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, ReturnType};
use uuid::Uuid;

use crate::utils::AttributesExt;
use crate::FromMeta;

#[derive(Debug, FromMeta)]
pub struct RouteMeta {
    group: Option<String>,
    id: Option<String>,
}

pub(crate) fn request_handler(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let meta = args;
    let group = if let Some(group_name) = meta.group {
        quote! { Some(#group_name) }
    } else {
        quote! { None }
    };
    let mut input = parse_macro_input!(input_stream as ItemFn);

    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();

    let operation_id = meta.id.unwrap_or(fn_ident_string.clone());

    let docs = match input.attrs.get_doc() {
        None => {
            quote!(None)
        }
        Some(t) => {
            quote! { Some(#t) }
        }
    };

    let random_uuid = Uuid::new_v4().simple().to_string();
    let uuid_ident = format_ident!("__PARAM_{}", random_uuid);
    let ret_uuid_ident = format_ident!("__RET_{}", random_uuid);
    let params_token: Vec<proc_macro2::TokenStream> = input
        .sig
        .inputs
        .iter()
        .flat_map(|param| match param {
            FnArg::Receiver(_) => None,
            FnArg::Typed(typed) => {
                // Check if the parameter has the #[api(skip)] attribute
                let should_skip = typed.attrs.iter().any(|attr| {
                    if attr.path.is_ident("api") {
                        // Parse the attribute tokens to check for "skip"
                        let tokens = attr.tokens.to_string();
                        tokens.contains("skip")
                    } else {
                        false
                    }
                });

                if should_skip {
                    return None;
                }
                let ty = &typed.ty;
                Some(quote! { Box::new(|path:String| {<#ty as ::gotcha::ParameterProvider>::generate(path) }) })
            }
        })
        .collect();
    let ret_pos = &input.sig.output;
    let ret_schematic = match ret_pos {
        ReturnType::Default => {
            quote! {
                Box::new(|| { ( () as ::gotcha::Responsible).response() })
            }
        }

        ReturnType::Type(_, ty) => {
            quote! {
                Box::new(|| {<#ty as ::gotcha::Responsible>::response()})
            }
        }
    };

    input.sig.inputs.iter_mut().for_each(|param| {
        if let FnArg::Typed(typed) = param {
            // Remove only the #[api(...)] attributes, keep others
            typed.attrs.retain(|attr| !attr.path.is_ident("api"));
        }
    });

    let ret = quote! {

        #input

        static #uuid_ident : ::gotcha::Lazy<Vec<Box<dyn Fn(String) -> ::gotcha::Either<Vec<::gotcha::oas::Parameter>, ::gotcha::oas::RequestBody> + Send + Sync + 'static>>> = ::gotcha::Lazy::new(||{
                    vec![
                    #( #params_token , )*
                ]
                });
        static #ret_uuid_ident : ::gotcha::Lazy<Box<dyn Fn() -> ::gotcha::oas::Responses + Send + Sync + 'static>> = ::gotcha::Lazy::new(||{
            #ret_schematic
        });
        ::gotcha::inventory::submit! {
            ::gotcha::Operable {
                type_name: concat!(module_path!(), "::", #fn_ident_string),
                id: #operation_id,
                group: #group,
                description: #docs,
                deprecated: false,
                parameters: &#uuid_ident,
                responses: &#ret_uuid_ident,
            }
        }
    };
    TokenStream::from(ret)
}
