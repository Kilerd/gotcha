use gotcha::tracing::info;
use gotcha::{api, get, GotchaApp, Path};
use gotcha::{post, put, Json, GotchaCli, Responder, Schematic};
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
#[api(id="index", group="hello")]
pub async fn hello_world() -> impl Responder {
    "hello world"
}

/// Add new pet to the store inventory.
#[api]
pub async fn new_pet() -> impl Responder {
    "new pet"
}

/// Update specific pet's info
#[api()]
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
    pet_type: PetType,
}


/// Update specific pet's address
pub async fn update_pet_address_detail(_paths: Path<UpdatePetAddressPathArgs>, _payload: Json<PetUpdateJson>) -> impl Responder {
    "update pet info"
}

#[derive(Debug, Deserialize, Clone)]
struct Config {}

#[tokio::main]
async fn main() {
    GotchaApp::<_, Config>::new()
        .get("/", hello_world)
        .post("/pets", new_pet)
        .put("/pets/:pet_id", update_pet_info)
        .put("/pets/{pet_id}/address/{address_id}", update_pet_address_detail)
        .done()
        .serve("0.0.0.0", 8080)
        .await
}
