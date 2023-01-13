use darling::FromMeta;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse_macro_input, AttributeArgs, ItemFn, LitStr};

#[proc_macro_attribute]
pub fn get(args: TokenStream, input_stream: TokenStream) -> TokenStream {
    let attr_args = parse_macro_input!(args as AttributeArgs);

    let _args = match RouteMeta::from_list(&attr_args) {
        Ok(v) => v,
        Err(e) => {
            return TokenStream::from(e.write_errors());
        }
    };
    let input = parse_macro_input!(input_stream as ItemFn);

    let fn_ident = input.sig.ident.clone();
    let fn_ident_string = fn_ident.to_string();

    let RouteMeta { path, extra } = _args;
    let ret = quote! {
        #[::actix_web::get( "/" )]
        #input

        impl ::gotcha::wrapper::gotcha_lib::Operation for  #fn_ident {
            fn method(&self) -> ::actix_web::http::Method {
                ::actix_web::http::Method::GET
            }
            fn uri(&self) -> &'static str {
                #path
            }
            fn summary(&self) -> &'static str {
                #fn_ident_string
            }
        }
    };
    println!("parsed: {}", ret.to_string());
    TokenStream::from(ret)
}

#[derive(Debug)]
struct RouteMeta {
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
