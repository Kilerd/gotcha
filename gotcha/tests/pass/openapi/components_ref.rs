//! Named schemas are emitted once under `components/schemas` and referenced by `$ref` at their
//! use sites, instead of being inlined at every use. This is what lets a recursive type produce a
//! finite spec at all.

use gotcha::{api, openapi::generate_openapi, Json, Schematic};
use serde::{Deserialize, Serialize};

/// A tree node that contains more of itself — inlining this never terminates.
#[derive(Schematic, Serialize, Deserialize)]
struct Node {
    name: String,
    children: Vec<Node>,
}

#[derive(Schematic, Serialize, Deserialize)]
struct User {
    id: u32,
}

/// Enums recurse too, and get the same treatment.
#[derive(Schematic, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum Expr {
    Literal { value: i32 },
    Sum { operands: Vec<Expr> },
}

#[api(id = "eval_expr")]
async fn eval_expr() -> Json<Expr> {
    unimplemented!()
}

#[api(id = "get_tree")]
async fn get_tree() -> Json<Node> {
    unimplemented!()
}

#[api(id = "get_user")]
async fn get_user() -> Json<User> {
    unimplemented!()
}

#[api(id = "list_users")]
async fn list_users() -> Json<User> {
    unimplemented!()
}

fn operable<H, T>(_handler: H) -> &'static gotcha::openapi::Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().expect("handler is registered")
}

fn main() {
    use gotcha::axum::http::Method;

    let mut operables = std::collections::HashMap::new();
    operables.insert(("/tree".to_string(), Method::GET), operable(get_tree));
    operables.insert(("/users/:id".to_string(), Method::GET), operable(get_user));
    operables.insert(("/users".to_string(), Method::GET), operable(list_users));
    operables.insert(("/eval".to_string(), Method::GET), operable(eval_expr));

    // Recursive `Node` must not hang or blow the stack here.
    let spec = generate_openapi(operables);
    let json = serde_json::to_value(&spec).unwrap();

    // Both named types are registered exactly once, under their `Schematic::name()`.
    let schemas = &json["components"]["schemas"];
    assert!(schemas.get("Node").is_some(), "Node is a component: {schemas}");
    assert!(schemas.get("User").is_some(), "User is a component: {schemas}");

    // The recursion is expressed as a self-reference, so the spec is finite.
    let children = &schemas["Node"]["properties"]["children"];
    assert_eq!(children["items"]["$ref"], "#/components/schemas/Node", "recursive field refs itself: {children}");

    // A type used by two endpoints is referenced, not duplicated inline.
    let body_ref = |path: &str| json["paths"][path]["get"]["requestBody"].clone();
    let _ = body_ref; // responses, not request bodies, for these handlers
    let user_response = |path: &str| json["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"].clone();
    assert_eq!(user_response("/users/{id}")["$ref"], "#/components/schemas/User");
    assert_eq!(user_response("/users")["$ref"], "#/components/schemas/User");

    // A recursive enum is finite for the same reason: the self-referential variant field refs it.
    assert!(schemas.get("Expr").is_some(), "enums are components too: {schemas}");
    let expr = serde_json::to_string(&schemas["Expr"]).unwrap();
    assert!(expr.contains("#/components/schemas/Expr"), "recursive enum refs itself: {expr}");

    // Primitives are not hoisted into components — only named (derived) types are.
    assert!(schemas.get("string").is_none(), "primitives stay inline: {schemas}");
    assert!(schemas.get("u32").is_none(), "primitives stay inline: {schemas}");
}
