use gotcha::{api, ConfigWrapper, GotchaApp, GotchaContext, GotchaRouter, Json, Path, Schematic};
use serde::{Deserialize, Serialize};


#[derive(Schematic, Serialize, Deserialize, Debug)]
pub struct ResponseWrapper<T: Schematic> {
    pub code: i32,
    pub message: String,
    pub data: T,
}

#[derive(Schematic, Serialize, Deserialize, Debug)]
pub struct Pet {
    pub id: i32,
    pub name: String,
    pub pet_type: PetType,
}

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
#[api(id = "index", group = "hello")]
pub async fn hello_world() -> &'static str {
    "hello world"
}

/// Add new pet to the store inventory.
#[api]
pub async fn new_pet() -> &'static str {
    "new pet"
}

#[api]
pub async fn get_pet(_paths: Path<(i32,)>) -> Json<Pet> {
    Json(Pet {
        id: 1,
        name: "dog".to_string(),
        pet_type: PetType::Cat,
    })
}

/// Update specific pet's info
#[api]
pub async fn update_pet_info(_paths: Path<(i32,)>) -> &'static str {
    "update pet info"
}

#[derive(Schematic, Deserialize)]
pub struct UpdatePetAddressPathArgs {
    pub pet_id: i32,
    pub address_id: String,
}

/// the world belongs to cat
#[derive(Schematic, Serialize, Deserialize, Debug)]
pub enum PetType {
    Cat,
    OtherCat,
    MoreCat,
}

/// the world belongs to cat
#[derive(Schematic, Deserialize)]
pub struct PetUpdateJson {
    /// new pet name
    name: String,
    pet_type: PetType,
}

/// Update specific pet's address
#[api]
pub async fn update_pet_address_detail(_paths: Path<UpdatePetAddressPathArgs>, _payload: Json<PetUpdateJson>) -> String {
    format!("update pet info: {} {:?}", _payload.name, _payload.pet_type)
}

#[derive(Debug, Deserialize, Clone, Serialize, Default)]
struct Config {}

struct App {}

impl GotchaApp for App {
    type State = ();
    type Config = Config;
    fn routes(&self, router: GotchaRouter<GotchaContext<(), Config>>) -> GotchaRouter<GotchaContext<(), Config>> {
        router
            .get("/", hello_world)
            .post("/pets", new_pet)
            .get("/pets/:pet_id", get_pet)
            .put("/pets/:pet_id", update_pet_info)
            .put("/pets/:pet_id/address/:address_id", update_pet_address_detail)
    }

    async fn state<'a, 'b>(&'a self, _config: &'b ConfigWrapper<Self::Config>) -> Result<Self::State, Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    App {}.run().await?;
    Ok(())
}
