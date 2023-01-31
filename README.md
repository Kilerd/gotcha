# gotcha
provide a featured web framework

## aim to
 - [x] everything of actix-web
 - [ ] automatically swagger api generation
 - [ ] built-in message mechanism
 - [x] environment based configuration system
 - [x] logging system
 - [ ] opt-in prometheus integration
 - [ ] sqlx based magic ORM
 - [ ] cron-based task system

## get started
add dependency into `Cargo.toml`
```toml
actix-web = "4"
gotcha = {version = "0.1"}
tokio = {version = "1", features = ["macros", 'rt-multi-thread']}
serde = {version="1", features=["derive"]}
```
```rust
use gotcha::{get, App, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder};
use serde::Deserialize;

#[get("/")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

#[tokio::main]
async fn main() {
    GotchaCli::<_, Config>::new()
        .server(|config| async move {
            HttpServer::new(|| {
                App::new()
                    .into_gotcha()
                    .service(hello_world)
                    .data(config)
                    .done()
            })
            .bind(("127.0.0.1", 8080))
                .unwrap()
                .run()
                .await;
        })
        .run()
        .await
}
```

## Conservator ORM

Conservator ORM is based on sqlx, currently it only support postgres

```rust
#[derive(Debug, Deserialize, Serialize, Crud, FromRow)]
#[crud(table = "users")]
pub struct UserEntity {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: UserRole,
    pub create_at: DateTime<Utc>,
    pub last_login_at: DateTime<Utc>,
}
```
the struct derived `Crud` would auto generate methods like:
- `find_by_id` return optional entity
- `fetch_one_by_id` return entity or raise
- `fetch_all` return all entities
- `create` passing the `Createable` to insert into table

```rust 
#[derive(Debug, Deserialize, Serialize, Creatable)]
pub struct NewUser {
    pub username: String,
    pub email: String,
    pub password: String,
}
```

`Createable` means it can be executed by magic ORM, using `UserEntity::create(NewUser{...})` to create a new user into
user table.

Conservator ORM aslo provide the `#[magic]` proc macro for those customized sql query.
```rust
use conservator::auto;
impl UserEntity {
    #[auto]
    pub async fn find_by__email__is<E>(email: &str, executor: E) -> Result<Option<UserEntity>, Error> {
        todo!()
    }

    #[auto]
    pub async fn exists_by_email_is<E>(_email: &str, executor: E) -> Result<bool, Error> {
        todo!()
    }
}
```
code above will generate two sql query statement automatically:
 - `select * from users where email = $1`
 - `select exists(select 1 from users where email = $1)`

and `#[sql]` aslo provide some convinent way to write customized sql query
```rust
use conservator::sql;

impl UserEntity {
    
    #[sql(find)]
    pub async fn find_user<E>(email: &str, executor: E) -> Result<Option<UserEntity>, Error> {
        "select * from users where email = :email"
    }
}
```
notice that, rather than sqlx's `$1`, we use param `:email` in sql, it can be used in native sql execution tools as well without any modification, like IDEA.

