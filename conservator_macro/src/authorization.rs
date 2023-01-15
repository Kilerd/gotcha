// use proc_macro2::TokenStream;
// use quote::{format_ident, quote, ToTokens};
// use syn::{AttributeArgs, FnArg, ItemFn};

// pub(crate) fn handler(args: AttributeArgs, func: ItemFn) -> TokenStream {
//     let vis = &func.vis;
//     let ident = &func.sig.ident;
//     let generics = &func.sig.generics;

//     let input = &func.sig.inputs;
//     let request_field_name = input
//         .iter()
//         .find(|it| match it {
//             FnArg::Receiver(_) => false,
//             FnArg::Typed(typed) => typed.ty.to_token_stream().to_string().eq("HttpRequest"),
//         })
//         .and_then(|it| match it {
//             FnArg::Receiver(_) => None,
//             FnArg::Typed(typed) => Some(typed.pat.to_token_stream().to_string()),
//         });

//     let http_request = if request_field_name.is_some() {
//         quote! {}
//     } else {
//         quote! { http_request: ::actix_web::HttpRequest, }
//     };

//     let fields: Vec<TokenStream> = input
//         .iter()
//         .filter_map(|it| match it {
//             FnArg::Receiver(_) => None,
//             FnArg::Typed(typed) => Some(typed.pat.to_token_stream()),
//         })
//         .collect();

//     let request_name = request_field_name
//         .map(|it| format_ident!("{}", it))
//         .unwrap_or(format_ident!("http_request"));

//     let output = &func.sig.output;
//     let func_block = &func.block;

//     let c = quote! {
//         #vis async fn #ident #generics (#http_request #input) -> impl ::actix_web::Responder {
//             use ::actix_web::ResponseError;
//             async fn __inner #generics(#input) #output #func_block

//             static PERMISSIONS: ::phf::Set<&str> = ::phf::phf_set! { #(#args,)* };

//             let permission = {
//                 let x = #request_name .req_data();
//                 x.get::<crate::domain::user::UserEntity>().map(|it|it.role.as_ref().to_string())
//             };
//             if let Some(user_permission) = permission {
//                 if PERMISSIONS.contains(&user_permission) {
//                     let x1 = __inner( #(#fields,)* ).await;
//                     ::actix_web::Either::Left(x1)
//                 } else {
//                     ::actix_web::Either::Right(crate::infrastructure::error::AppError::PermissionDeny.error_response())
//                 }
//             } else {
//                 ::actix_web::Either::Right(crate::infrastructure::error::AppError::AuthenticationRequired.error_response())
//             }

//         }
//     };
//     c
// }