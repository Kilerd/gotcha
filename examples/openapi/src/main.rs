use gotcha::tracing::info;
use gotcha::web::Path;
use gotcha::{get, post, put, App, web::{Json}, GotchaAppWrapperExt, GotchaCli, HttpServer, Responder, Schematic};
use serde::Deserialize;

/// Rust has six types of attributes.
///
/// - Outer attributes like `#[repr(transparent)]`. These appear outside or in front of the item they describe.
/// - Inner attributes like `#![feature(proc_macro)]`. These appear inside of the item they describe, usually a module.
/// - Outer doc comments like /// # Example.
/// - Inner doc comments like //! Please file an issue.
/// - Outer block comments /** # Example */.
/// - Inner block comments /*! Please file an issue */.
/// - The style field of type AttrStyle distinguishes whether an attribute is outer or inner. Doc comments and block comments are promoted to attributes, as this is how they are processed by the compiler and by macro_rules! macros.
///
/// The path field gives the possibly colon-delimited path against which the attribute is resolved. It is equal to "doc" for desugared doc comments. The tokens field contains the rest of the attribute body as tokens.
/// ```shell
/// #[derive(Copy)]      #[crate::precondition x < 5]
///   ^^^^^^~~~~~~         ^^^^^^^^^^^^^^^^^^^ ~~~~~
///   path  tokens                 path        tokens
/// ```
#[get("/")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

/// Add new pet to the store inventory.
#[post("/pet", group = "pet")]
pub async fn new_pet() -> impl Responder {
    "new pet"
}

/// Update specific pet's info
#[put("/pets/{pet_id}/info")]
pub async fn update_pet_info(_paths: Path<(i32,)>) -> impl Responder {
    "update pet info"
}

#[derive(Schematic, Deserialize)]
pub struct UpdatePetAddressPathArgs {
    pub pet_id: i32,
    pub address_id: String,
}

/// the world belongs to cat
#[derive(Schematic, Deserialize)]
pub enum PetType {
    Cat,
    OtherCat,
    MoreCat,
}

/// the world belongs to cat
#[derive(Schematic, Deserialize)]
pub struct PetUpdateJson {
    name: String,
    pet_type: PetType
}




/// Update specific pet's address
#[put("/pets/{pet_id}/address/{address_id}")]
pub async fn update_pet_address_detail(_paths: Path<UpdatePetAddressPathArgs>, _payload: Json<PetUpdateJson>) -> impl Responder {
    "update pet info"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

#[tokio::main]
async fn main() {
    GotchaCli::<_, Config>::new()
        .server(|config| async move {
            info!("starting application");
            HttpServer::new(move || {
                App::new()
                    .into_gotcha()
                    .service(hello_world)
                    .service(new_pet)
                    .service(update_pet_info)
                    .service(update_pet_address_detail)
                    .data(config.clone())
                    .done()
            })
            .workers(6)
            .bind(("0.0.0.0", 8080))
            .unwrap()
            .run()
            .await
        })
        .run()
        .await
}
