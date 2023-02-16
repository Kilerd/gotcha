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
    let ident = crud_opts.ident.clone();
    let ident_string = ident.to_string();

    // todo handle enum
    let fields = crud_opts.data.take_struct().unwrap();
    let fields_stream: Vec<proc_macro2::TokenStream> = fields
        .fields
        .into_iter()
        .map(|field| {

            let field_name = field.ident.unwrap().to_string();
            let field_ty = field.ty;
            quote!{
                properties.insert(#field_name.to_string(), #field_ty::generate_schema().to_value());
            }

        })
        .collect();
    let impl_stream = quote! {
        
        impl ApiObject for #ident {
            fn name() -> &'static str {
                #ident_string
            }
        
            fn required() -> bool {
                true
            }
        
            fn type_() -> &'static str {
                "object"
            }
            fn generate_schema() -> Schema {
                let mut schema = Schema{
                    _type: Some(Self::type_().to_string()),
                    format:None,
                    nullable:None,
                    extras:Default::default()
                };
                let mut properties = ::std::collections::HashMap::new();
                #(
                    #fields_stream
                )*
                schema.extras.insert("properties".to_string(), ::serde_json::to_value(properties).unwrap());
                schema
            }
        }
    };
    dbg!(impl_stream.to_string());
    Ok(impl_stream)
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

