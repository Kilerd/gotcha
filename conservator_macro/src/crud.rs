use darling::FromDeriveInput;

use quote::quote;
use syn::{parse2, DeriveInput};

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(crud))]
struct CrudOpts {
    ident: syn::Ident,
    table: String,
}

fn find_by_id(table_name: &str) -> String {
    format!("select * from {} where id = $1", table_name)
}
fn fetch_all(table_name: &str) -> String {
    format!("select * from {}", table_name)
}

pub(crate) fn handler(input: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let x1 = parse2::<DeriveInput>(input).unwrap();
    let crud_opts: CrudOpts = CrudOpts::from_derive_input(&x1).unwrap();

    let table_name = &crud_opts.table;
    let ident = crud_opts.ident;

    let find_by_id_sql = find_by_id(&crud_opts.table);
    let fetch_all_sql = fetch_all(&crud_opts.table);

    quote! {
        #[async_trait::async_trait]
        impl ::conservator::Crud for #ident {
            type PrimaryKey = Uuid;

            fn table_name() -> &'static str {
                #table_name
            }

            async fn find_by_id<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database=::sqlx::Postgres>>(pk: &Uuid, executor: E) -> Result<Option<Self>, ::sqlx::Error> {
                sqlx::query_as(#find_by_id_sql)
                .bind(pk)
                .fetch_optional(executor)
                .await
            }

            async fn fetch_one_by_id<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database=::sqlx::Postgres>>(pk: &Uuid, executor: E) -> Result<Self, ::sqlx::Error> {
                sqlx::query_as(#find_by_id_sql)
                .bind(pk)
                .fetch_one(executor)
                .await
            }

            async fn fetch_all<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database=::sqlx::Postgres>>(executor: E) -> Result<Vec<Self>, ::sqlx::Error> {
                sqlx::query_as(#fetch_all_sql)
                .fetch_all(executor)
                .await
            }
            async fn create<'e, 'c: 'e, E: 'e + ::sqlx::Executor<'c, Database = ::sqlx::Postgres>, C: ::conservator::Creatable>(
                data: C, executor: E,
            ) -> Result<Self, ::sqlx::Error> {
                let sql = format!("INSERT INTO {} {} returning *", #table_name, data.get_insert_sql());
                let mut ex = sqlx::query_as(&sql);
                data.build(ex)
                    .fetch_one(executor)
                    .await
            }

        }

    }
}
