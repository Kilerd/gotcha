use quote::quote;

use itertools::Itertools;
use quote::format_ident;
use std::str::FromStr;
use strum::EnumString;
use syn::{parse2, ImplItem, ImplItemMethod, Item, Type};

#[derive(Debug, EnumString)]
#[strum(serialize_all = "snake_case")]
enum Action {
    Fetch,
    Exists,
    Find,
    FetchAll,
}

impl Action {
    fn as_sql(&self, where_sql: &str) -> String {
        match self {
            Action::Fetch => {
                format!("select * from {{}} where {}", where_sql)
            }
            Action::Exists => {
                format!("select exists(select 1 from {{}} where {})", where_sql)
            }
            Action::Find => {
                format!("select * from {{}} where {}", where_sql)
            }
            Action::FetchAll => {
                format!("select * from {{}} where {}", where_sql)
            }
        }
    }
    fn build_sqlx_query(&self, bind: Vec<proc_macro2::TokenStream>) -> proc_macro2::TokenStream {
        match self {
            Action::Fetch => {
                quote! {
                    sqlx::query_as(&sql)
                    #(#bind)*
                    .fetch_one(executor)
                    .await
                }
            }
            Action::Exists => {
                quote! {
                    Ok(sqlx::query_as::<_, (bool, )>(&sql)
                    #(#bind)*
                    .fetch_one(executor)
                    .await?.0)
                }
            }
            Action::Find => {
                quote! {
                    sqlx::query_as(&sql)
                    #(#bind)*
                    .fetch_optional(executor)
                    .await
                }
            }
            Action::FetchAll => {
                quote! {
                    sqlx::query_as(&sql)
                    #(#bind)*
                    .fetch_all(executor)
                    .await
                }
            }
        }
    }
}

#[derive(Debug, EnumString)]
#[strum(serialize_all = "snake_case")]
enum Factor {
    Is,
    Equals,
    Gt,
    Lt,
    In,
}

impl Factor {
    fn as_sql_factor(&self) -> String {
        match self {
            Factor::Is => "=".to_string(),
            Factor::Equals => "=".to_string(),
            Factor::Gt => ">".to_string(),
            Factor::Lt => "<".to_string(),
            Factor::In => {
                todo!()
            }
        }
    }
}

#[derive(Debug)]
struct FieldFactor {
    index: usize,
    field: String,
    factor: Factor,
}

impl FieldFactor {
    fn as_sql_where(&self) -> String {
        format!("{} {} ${}", self.field, self.factor.as_sql_factor(), self.index + 1)
    }
}

pub(crate) fn handler(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let x1 = parse2::<Item>(input).unwrap();
    if let Item::Impl(ref impl_body) = x1 {
        let header = &impl_body.self_ty;

        let items: Vec<proc_macro2::TokenStream> = impl_body
            .items
            .iter()
            .map(|item| match item {
                ImplItem::Method(method) => handler_magic_function(header, method),
                _ => {
                    quote! { #item }
                }
            })
            .collect();

        let a = quote! {
            impl #header {
                #(
                    #items
                )*

            }
        };
        a
    } else {
        quote! {}
    }
}

pub(crate) fn handler_magic_function(header: &Box<Type>, method: &ImplItemMethod) -> proc_macro2::TokenStream {
    let vis = &method.vis;
    let ident = &method.sig.ident;
    let ident_name = method.sig.ident.to_string();
    let action_naming_convention =
        regex::Regex::new("^(?P<action>fetch|fetch_all|exists|find)_by").expect("invalid regex");
    let action_caps = action_naming_convention
        .captures(&ident_name)
        .expect("action not found");

    let action_name = action_caps.name("action").map_or("", |m| m.as_str());
    let action: Action = Action::from_str(action_name).unwrap();
    let field_naming_convention =
        regex::Regex::new("__(?P<field>[^__]+)__(?P<factor>is|equals|gt|lt|in)?").expect("invalid regex");
    let field_factors = field_naming_convention
        .captures_iter(&ident_name)
        .into_iter()
        .enumerate()
        .map(|(idx, cap)| {
            let field_name = cap.name("field").map_or("", |m| m.as_str());
            let factor_name = cap.name("factor").map_or("", |m| m.as_str());
            FieldFactor {
                index: idx,
                factor: Factor::from_str(factor_name).unwrap(),
                field: field_name.to_string(),
            }
        })
        .collect::<Vec<_>>();

    let sql_where = field_factors.iter().map(|it| it.as_sql_where()).join(" and ");
    let ident_list = field_factors
        .iter()
        .map(|it| {
            let ident = format_ident!("{}", it.field);
            quote! { .bind(#ident)}
        })
        .collect::<Vec<_>>();
    // let sql_where = field_factors.iter().map(|it| it.as_sql_where()).join(" and ");

    let input = &method.sig.inputs;
    let output = &method.sig.output;

    let sql = action.as_sql(&sql_where);
    let sqlx_expression = action.build_sqlx_query(ident_list);

    quote! {
        #vis async fn #ident<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database=::sqlx::Postgres>>(#input) #output {
            let table_name = #header::table_name();
            let sql = format!(#sql, table_name);
            #sqlx_expression
        }
    }
}