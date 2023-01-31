use proc_macro::TokenStream;
use proc_macro_error::{proc_macro_error, abort};
use syn::AttributeArgs;
use syn::{parse_macro_input, ItemFn};

mod authorization;
mod creatable;
mod crud;
mod magic;
mod sql;
mod auto;

#[proc_macro_derive(Crud, attributes(crud))]
pub fn derive_crud_fn(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    let stream1 = proc_macro::TokenStream::from(crud::handler(stream2));
    stream1
}

#[proc_macro_derive(Creatable)]
#[proc_macro_error]
pub fn derive_creatable_fn(input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    let stream1 = proc_macro::TokenStream::from(creatable::handle_creatable(stream2));
    stream1
}

#[proc_macro_attribute]
pub fn magic(_args: TokenStream, input: TokenStream) -> TokenStream {
    let stream2 = proc_macro2::TokenStream::from(input);
    let stream1 = proc_macro::TokenStream::from(magic::handler(stream2));
    stream1
    // let args = parse_macro_input!(args as Args);
    // let mut item = parse_macro_input!(input as Item);
    // TokenStream::from(quote!(#item))
}

#[proc_macro_attribute]
#[proc_macro_error]
pub fn sql(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = proc_macro2::TokenStream::from(args);
    let stream2 = proc_macro2::TokenStream::from(input);
    match sql::handler(args, stream2) {
        Ok(stream) => proc_macro::TokenStream::from(stream),
        Err((span, msg)) => abort!{span, msg}
    }
    // let mut item = parse_macro_input!(input as Item);
    // TokenStream::from(quote!(#item))
}

// #[proc_macro_attribute]
// pub fn authorization(args: TokenStream, input: TokenStream) -> TokenStream {
//     let attr_args = parse_macro_input!(args as AttributeArgs);
//     let input = parse_macro_input!(input as ItemFn);
//     let stream1 = proc_macro::TokenStream::from(authorization::handler(attr_args, input));
//     stream1
// }


