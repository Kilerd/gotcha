use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, Lit, LitStr, Meta};
use uuid::Uuid;
use crate::FromMeta;
use crate::utils::AttributesExt;


#[derive(Debug)]
pub struct RouteMeta {
    extra: RouteExtraMeta,
}

#[derive(Debug, FromMeta)]
struct RouteExtraMeta {
    group: Option<String>,
    id: Option<String>
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
    let meta = args;
    let group = if let Some(group_name) = meta.extra.group {
        quote! { Some(#group_name) }
    } else {
        quote! { None }
    };
    let input = parse_macro_input!(input_stream as ItemFn);
    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();

    let operation_id = meta.extra.id.unwrap_or(fn_ident_string.clone());

    let docs = match input.attrs.get_doc() {
        None => { quote!(None) }
        Some(t) => { quote! { Some(#t) } }
    };

    let random_uuid = Uuid::new_v4().simple().to_string();
    let uuid_ident = format_ident!("__PARAM_{}", random_uuid);
    let params_token: Vec<proc_macro2::TokenStream> = input
            .sig
            .inputs
            .iter()
            .flat_map(|param| match param {
                FnArg::Receiver(_) => None,
                FnArg::Typed(typed) => {
                    let ty = &typed.ty;
                    Some(quote! { Box::new(|path:String| {<#ty as ::gotcha::ParameterProvider>::generate(path) }) })
                }
            })
            .collect();

    let ret = quote! {

        #input

        static #uuid_ident : ::gotcha::Lazy<Vec<Box<dyn Fn(String) -> ::gotcha::Either<Vec<::gotcha::oas::Parameter>, ::gotcha::oas::RequestBody> + Send + Sync + 'static>>> = ::gotcha::Lazy::new(||{
                    vec![
                    #( #params_token , )*
                ]
                });
        ::gotcha::inventory::submit! {
            ::gotcha::Operable {
                type_name: concat!(module_path!(), "::", #fn_ident_string),
                id: #operation_id,
                group: #group,
                description: #docs,
                deprecated: false,
                parameters: &#uuid_ident

            }
        }
    };
    let stream = TokenStream::from(ret);
    println!("{}", &stream);
    stream
}
