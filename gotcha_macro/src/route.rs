use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, FnArg, ItemFn, Lit, LitStr, Meta};

use crate::FromMeta;
use crate::utils::AttributesExt;

pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Options,
    Head,
    Connect,
    Trace,
}

impl HttpMethod {
    fn to_token_stream(&self) -> proc_macro2::TokenStream {
        match self {
            HttpMethod::Get => {
                quote! { ::gotcha::actix_web::http::Method::GET }
            }
            HttpMethod::Post => {
                quote! { ::gotcha::actix_web::http::Method::POST }
            }
            HttpMethod::Put => {
                quote! { ::gotcha::actix_web::http::Method::PUT }
            }
            HttpMethod::Patch => {
                quote! { ::gotcha::actix_web::http::Method::PATCH }
            }
            HttpMethod::Delete => {
                quote! { ::gotcha::actix_web::http::Method::DELETE }
            }
            HttpMethod::Options => {
                quote! { ::gotcha::actix_web::http::Method::OPTIONS }
            }
            HttpMethod::Head => {
                quote! { ::gotcha::actix_web::http::Method::HEAD }
            }
            HttpMethod::Connect => {
                quote! { ::gotcha::actix_web::http::Method::CONNECT }
            }
            HttpMethod::Trace => {
                quote! { ::gotcha::actix_web::http::Method::Trace }
            }
        }
    }
    fn to_guard_method(&self) -> proc_macro2::TokenStream {
        match self {
            HttpMethod::Get => {
                quote! { ::gotcha::actix_web::guard::Get() }
            }
            HttpMethod::Post => {
                quote! { ::gotcha::actix_web::guard::Post() }
            }
            HttpMethod::Put => {
                quote! { ::gotcha::actix_web::guard::Put() }
            }
            HttpMethod::Patch => {
                quote! { ::gotcha::actix_web::guard::Patch() }
            }
            HttpMethod::Delete => {
                quote! { ::gotcha::actix_web::guard::Delete() }
            }
            HttpMethod::Options => {
                quote! { ::gotcha::actix_web::guard::Options() }
            }
            HttpMethod::Head => {
                quote! { ::gotcha::actix_web::guard::Head() }
            }
            HttpMethod::Connect => {
                quote! { ::gotcha::actix_web::guard::Connect() }
            }
            HttpMethod::Trace => {
                quote! { ::gotcha::actix_web::guard::Trace() }
            }
        }
    }
}

#[derive(Debug)]
pub struct RouteMeta {
    path: LitStr,
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
        if items.len() == 0 {
            panic!("path must be set");
        }
        if !matches!(items[0], syn::NestedMeta::Lit(..)) {
            panic!("first param must be literal");
        }
        let path = match &items[0] {
            syn::NestedMeta::Lit(literal) => match literal {
                syn::Lit::Str(token) => token.clone(),
                _ => return Err(darling::Error::unexpected_type("other literal")),
            },
            _ => return Err(darling::Error::unexpected_type("not literal")),
        };
        let extra_meta = RouteExtraMeta::from_list(&items[1..])?;

        Ok(RouteMeta { path, extra: extra_meta })
    }
}

pub(crate) fn request_handler(method: HttpMethod, args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let RouteMeta { path, extra } = args;
    let group = if let Some(group_name) = extra.group {
        quote! { Some(#group_name.to_string()) }
    } else {
        quote! { None }
    };
    let should_generate_openapi_spec = !extra.disable_openapi.unwrap_or(false);
    let gurad_method = method.to_guard_method();
    let method = method.to_token_stream();
    let input = parse_macro_input!(input_stream as ItemFn);
    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();
    let docs = match input.attrs.get_doc() {
        None => {quote!(None)}
        Some(t) => {quote! { Some(#t) }}
    };
    let params_token: Vec<proc_macro2::TokenStream> = input
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
        .collect();

    let ret = quote! {

        #[allow(non_camel_case_types, missing_docs)]
        pub struct #fn_ident;

        impl ::gotcha::actix_web::dev::HttpServiceFactory for #fn_ident {
            fn register(self, __config: &mut ::gotcha::actix_web::dev::AppService) {

                #input
                let __resource = ::gotcha::actix_web::Resource::new(
                        #path,
                    )
                    .name(#fn_ident_string)
                    .guard(#gurad_method)
                    .to(#fn_ident);
                ::gotcha::actix_web::dev::HttpServiceFactory::register(__resource, __config);
            }
        }

        impl ::gotcha::Operable for  #fn_ident {
            fn should_generate_openapi_spec(&self) -> bool {
                #should_generate_openapi_spec
            }
            fn id(&self) -> &'static str {
                #fn_ident_string
            }
            fn method(&self) -> ::gotcha::actix_web::http::Method {
                #method
            }
            fn uri(&self) -> &'static str {
                #path
            }
            fn group(&self) -> Option<String> {
                #group
            }
            fn description(&self) -> Option<&'static str> {
                #docs
            }
            fn deprecated(&self) -> bool {
                false
            }
            fn parameters(&self) -> Vec<::gotcha::actix_web::Either<Vec<::gotcha::oas::Parameter>, ::gotcha::oas::RequestBody>> {
                let mut ret = vec![];

                #(
                    ret.push(#params_token);
                )*
                ret
            }
        }
    };
    TokenStream::from(ret)
}
