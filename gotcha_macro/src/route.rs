use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, Lit, LitStr, Meta};

use crate::FromMeta;
use crate::utils::AttributesExt;



#[derive(Debug)]
pub struct RouteMeta {
    extra: RouteExtraMeta,
}

#[derive(Debug, FromMeta)]
struct RouteExtraMeta {
    group: Option<String>,
    #[darling(default)]
    disable_openapi: Option<bool>,
}

impl FromMeta for RouteMeta {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        let extra_meta = RouteExtraMeta::from_list(&items)?;
        Ok(RouteMeta { extra: extra_meta })
    }
}

pub(crate) fn request_handler(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let RouteMeta { extra } = args;
    let group = if let Some(group_name) = extra.group {
        quote! { Some(#group_name) }
    } else {
        quote! { None }
    };
    let should_generate_openapi_spec = !extra.disable_openapi.unwrap_or(false);
    let input = parse_macro_input!(input_stream as ItemFn);
    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();
    let docs = match input.attrs.get_doc() {
        None => { quote!(None) }
        Some(t) => { quote! { Some(#t) } }
    };
    let params_token: Vec<proc_macro2::TokenStream> = if should_generate_openapi_spec {
        input
            .sig
            .inputs
            .iter()
            .flat_map(|param| match param {
                FnArg::Receiver(_) => None,
                FnArg::Typed(typed) => {
                    let ty = &typed.ty;
                    Some(quote! { <#ty as ::gotcha::ParameterProvider>::generate(self.uri().to_string())})
                }
            })
            .collect()
    } else { Vec::new() };

    let ret = quote! {

        #input

        ::gotcha::inventory::submit! {
            ::gotcha::Operable {
                id: concat!(module_path!(), "::", #fn_ident_string),
                group: #group,
                description: #docs,
                deprecated: false,
                parameters: vec![
                    #( #params_token, )*
                ]
            }
        }
    };
    let stream = TokenStream::from(ret);
    println!("{}", &stream);
    stream
}
