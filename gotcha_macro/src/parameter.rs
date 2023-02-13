use darling::{FromDeriveInput, FromField, FromVariant};

use proc_macro2::Span;
use quote::quote;
use syn::{parse2, spanned::Spanned, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(parameter))]
struct ParameterOpts {
    ident: syn::Ident,
    // fall over the serde info
    data: darling::ast::Data<darling::util::Ignored, ParameterStructFieldOpt>,
}

#[derive(Debug, FromField)]
#[darling(attributes(parameter))]
struct ParameterStructFieldOpt {
    ident: Option<syn::Ident>,
    ty: syn::Type,
    // add more validator
    #[darling(default)]
    primary_key: Option<bool>,
}

pub(crate) fn handler(
    input: proc_macro2::TokenStream,
) -> Result<proc_macro2::TokenStream, (Span, &'static str)> {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let crud_opts: ParameterOpts = ParameterOpts::from_derive_input(&x1).unwrap();

    dbg!(&crud_opts);

    // todo handle enum
    let fields = crud_opts.data.take_struct().unwrap();
    // let mut pk_count = fields
    //     .fields
    //     .into_iter()
    //     .filter(|field| field.primary_key == Some(true))
    //     .collect_vec();

    Ok(quote! {

    })
}


#[cfg(test)]
mod test {
    use quote::quote;
    use crate::parameter::handler;

    #[test]
    fn pass() {
        let ret = quote!(
            #[derive(Parameter)]
            pub struct PaginationRequest {
                page: usize,
                size: usize
            }
        );
        handler(ret);
    }
}
