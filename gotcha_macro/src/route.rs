use proc_macro::TokenStream;
use crate::FromMeta;
use quote::quote;
use syn::{AttributeArgs, FnArg, ItemFn, Lit, LitStr, Meta, parse_macro_input};

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
            HttpMethod::Get => { quote! { ::actix_web::http::Method::GET } }
            HttpMethod::Post => { quote! { ::actix_web::http::Method::POST } }
            HttpMethod::Put => { quote! { ::actix_web::http::Method::PUT } }
            HttpMethod::Patch => { quote! { ::actix_web::http::Method::PATCH } }
            HttpMethod::Delete => { quote! { ::actix_web::http::Method::DELETE } }
            HttpMethod::Options => { quote! { ::actix_web::http::Method::OPTIONS } }
            HttpMethod::Head => { quote! { ::actix_web::http::Method::HEAD } }
            HttpMethod::Connect => { quote! { ::actix_web::http::Method::CONNECT } }
            HttpMethod::Trace => { quote! { ::actix_web::http::Method::Trace } }
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

        Ok(RouteMeta {
            path,
            extra: extra_meta,
        })
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
    let method = method.to_token_stream();

    let input = parse_macro_input!(input_stream as ItemFn);
    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();
    let docs: Vec<String> = input.attrs.iter().filter_map(|attr| {
        match attr.parse_meta().unwrap() {
            Meta::NameValue(doc) => { if doc.path.is_ident("doc") { Some(doc) } else { None } }
            _ => None
        }
    }).filter_map(|attr| match attr.lit {
        Lit::Str(lit_str) => Some(lit_str.value()),
        _ => {
            None
        }
    }).map(|doc| doc.trim().to_string()).collect();

    let docs = if docs.is_empty() { quote!(None)} else {
        let t = docs.join("\n");
        quote!{ Some(#t) }
    };
    let params_token: Vec<proc_macro2::TokenStream> = input.sig.inputs.iter().flat_map(|param| match param {
        FnArg::Receiver(_) => None,
        FnArg::Typed(typed) => {
            let ty = &typed.ty;
            Some(quote!{ <#ty as ParameterProvider>::generate(self.uri().to_string())})
        }
    }).collect();

    let ret = quote! {
        #[::actix_web::get( "/" )]
        #input

        impl Operable for  #fn_ident {
            fn id(&self) -> &'static str {
                #fn_ident_string
            }
            fn method(&self) -> ::actix_web::http::Method {
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
            fn parameters(&self) -> Vec<Parameter> {
                let mut ret = vec![];

                #(
                    if let Some(mut one_params) = #params_token {
                        ret.append(&mut one_params);
                    }

                )*
                ret

            }
        }
    };
    TokenStream::from(ret)
}