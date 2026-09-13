#![cfg(feature = "openapi")]
#![allow(dead_code)]

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use gotcha::axum::http::Method;
use gotcha::{api, openapi::generate_openapi, Json, Schematic};
use gotcha_core::registry::collect;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod screens {
    use super::*;
    #[derive(Schematic, Serialize, Deserialize)]
    pub struct SendResult {
        pub screen_id: String,
        pub children: Vec<SendResult>,
    }
}
mod terminals {
    use super::*;
    #[derive(Schematic, Serialize, Deserialize)]
    pub struct SendResult {
        pub exit_code: u32,
    }
}
#[derive(Schematic, Serialize, Deserialize)]
struct Envelope<T: Schematic> {
    value: T,
}
#[derive(Schematic, Serialize, Deserialize)]
#[schematic(name = "ScreenSendResult")]
struct ExplicitResult {
    screen: screens::SendResult,
}

#[api]
async fn screen() -> Json<screens::SendResult> {
    unimplemented!()
}
#[api]
async fn terminal() -> Json<terminals::SendResult> {
    unimplemented!()
}
#[api]
async fn string_envelope() -> Json<Envelope<String>> {
    unimplemented!()
}
#[api]
async fn number_envelope() -> Json<Envelope<u32>> {
    unimplemented!()
}
#[api]
async fn explicit(body: Json<ExplicitResult>) -> Json<ExplicitResult> {
    body
}

fn operable<H, T>(_: H) -> &'static gotcha::openapi::Operable
where
    H: gotcha::axum::handler::Handler<T, ()>,
    T: 'static,
{
    gotcha::router::extract_operable::<H, T, ()>().unwrap()
}
fn assemble(reverse: bool) -> Value {
    let mut routes = vec![
        ("/screen", operable(screen)),
        ("/terminal", operable(terminal)),
        ("/string", operable(string_envelope)),
        ("/number", operable(number_envelope)),
        ("/explicit", operable(explicit)),
        ("/screen-again", operable(screen)),
    ];
    if reverse {
        routes.reverse();
    }
    let routes: HashMap<_, _> = routes.into_iter().map(|(path, op)| ((path.to_owned(), Method::POST), op)).collect();
    serde_json::to_value(generate_openapi(routes)).unwrap()
}
fn response<'a>(spec: &'a Value, path: &str) -> &'a Value {
    &spec["paths"][path]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
}

#[test]
fn assembly_separates_types_and_rewrites_every_reference_deterministically() {
    let spec = assemble(false);
    for reverse in [true, false, true] {
        assert_eq!(spec, assemble(reverse));
    }
    let schemas = &spec["components"]["schemas"];
    assert_eq!(schemas.as_object().unwrap().len(), 5);
    assert_eq!(schemas["ScreensSendResult"]["properties"]["screen_id"]["type"], "string");
    assert_eq!(schemas["TerminalsSendResult"]["properties"]["exit_code"]["type"], "integer");
    assert_eq!(
        schemas["ScreensSendResult"]["properties"]["children"]["items"]["$ref"],
        "#/components/schemas/ScreensSendResult"
    );
    assert_eq!(schemas["Envelope_String"]["properties"]["value"]["type"], "string");
    assert_eq!(schemas["Envelope_u32"]["properties"]["value"]["type"], "integer");
    assert_eq!(response(&spec, "/screen")["$ref"], "#/components/schemas/ScreensSendResult");
    assert_eq!(response(&spec, "/screen-again"), response(&spec, "/screen"));
    assert_eq!(response(&spec, "/terminal")["$ref"], "#/components/schemas/TerminalsSendResult");
    assert_eq!(response(&spec, "/string")["$ref"], "#/components/schemas/Envelope_String");
    assert_eq!(response(&spec, "/number")["$ref"], "#/components/schemas/Envelope_u32");
    assert_eq!(response(&spec, "/explicit")["$ref"], "#/components/schemas/ScreenSendResult");
    assert_eq!(
        spec["paths"]["/explicit"]["post"]["requestBody"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/ScreenSendResult"
    );
    assert_eq!(
        schemas["ScreenSendResult"]["properties"]["screen"]["$ref"],
        "#/components/schemas/ScreensSendResult"
    );
    assert!(!spec.to_string().contains("gotcha:rust-type:"));
    assert_eq!(ExplicitResult::name(), "ScreenSendResult");
}

#[test]
fn names_are_resolved_after_collection_in_either_order_and_scopes_stay_independent() {
    for reverse in [false, true] {
        let (refs, schemas) = collect(|| {
            if reverse {
                vec![terminals::SendResult::generate_schema(), screens::SendResult::generate_schema()]
            } else {
                vec![screens::SendResult::generate_schema(), terminals::SendResult::generate_schema()]
            }
        });
        let first = if reverse { "TerminalsSendResult" } else { "ScreensSendResult" };
        assert_eq!(refs[0].schema.extras["$ref"], format!("#/components/schemas/{first}"));
        assert_eq!(
            schemas.keys().map(String::as_str).collect::<Vec<_>>(),
            ["ScreensSendResult", "TerminalsSendResult"]
        );
    }
    let (schema, components) = collect(screens::SendResult::generate_schema);
    assert_eq!(schema.schema.extras["$ref"], "#/components/schemas/SendResult");
    assert_eq!(components.len(), 1);
    assert!(components.contains_key("SendResult"));
}

#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn capture(f: impl FnOnce()) -> String {
    let buffer = Capture(Arc::default());
    let writer = buffer.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, f);
    let bytes = buffer.0.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

// Tracing caches callsite interest globally. Run log assertions in their own process so
// concurrently entering/leaving subscribers in other tests cannot affect the capture.
fn isolate_log_test(name: &str) -> bool {
    if std::env::var("GOTCHA_SCHEMA_LOG_TEST").as_deref() == Ok(name) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("GOTCHA_SCHEMA_LOG_TEST", name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

#[test]
fn collisions_warn_with_original_types_and_final_names_but_reuse_does_not() {
    if isolate_log_test("collisions_warn_with_original_types_and_final_names_but_reuse_does_not") {
        return;
    }
    let logs = capture(|| {
        assemble(false);
    });
    assert_eq!(logs.matches("OpenAPI schema name collision").count(), 1, "{logs}");
    for expected in [
        "WARN",
        "screens::SendResult",
        "terminals::SendResult",
        "ScreensSendResult",
        "TerminalsSendResult",
    ] {
        assert!(logs.contains(expected), "missing {expected}: {logs}");
    }
    assert!(capture(|| {
        collect(|| vec![screens::SendResult::generate_schema(), screens::SendResult::generate_schema()]);
    })
    .is_empty());
}

mod overrides {
    use super::*;
    #[derive(Schematic)]
    #[schematic(name = "SendResult")]
    pub struct Reserved {
        field: bool,
    }
    pub mod left {
        use super::*;
        #[derive(Schematic)]
        #[schematic(name = "Duplicate")]
        pub struct First {
            field: String,
        }
    }
    pub mod right {
        use super::*;
        #[derive(Schematic)]
        #[schematic(name = "Duplicate")]
        pub struct Second {
            field: u32,
        }
    }
}
#[test]
fn explicit_names_are_reserved_and_duplicate_overrides_warn_without_losing_schemas() {
    if isolate_log_test("explicit_names_are_reserved_and_duplicate_overrides_warn_without_losing_schemas") {
        return;
    }
    let (_, schemas) = collect(|| vec![screens::SendResult::generate_schema(), overrides::Reserved::generate_schema()]);
    assert!(schemas.contains_key("SendResult"));
    assert!(schemas.contains_key("ScreensSendResult"));
    let logs = capture(|| {
        let (refs, schemas) = collect(|| vec![overrides::left::First::generate_schema(), overrides::right::Second::generate_schema()]);
        assert_eq!(schemas.len(), 2);
        assert_eq!(refs[0].schema.extras["$ref"], "#/components/schemas/LeftDuplicate");
        assert_eq!(refs[1].schema.extras["$ref"], "#/components/schemas/RightDuplicate");
    });
    assert!(logs.contains("Duplicate") && logs.contains("LeftDuplicate") && logs.contains("RightDuplicate"));
}

#[derive(Schematic)]
#[schematic(name = "PublicEnum")]
enum Simple {
    A,
    B,
}
#[derive(Schematic, Serialize)]
#[schematic(name = "PublicInternal")]
#[serde(tag = "kind")]
enum Internal {
    A { next: Vec<Internal> },
}
#[derive(Schematic, Serialize)]
#[schematic(name = "PublicAdjacent")]
#[serde(tag = "kind", content = "data")]
enum Adjacent {
    A(String),
}
#[derive(Schematic)]
#[schematic(name = "PublicExternal")]
enum External {
    A(String),
}
#[derive(Schematic, Serialize)]
#[schematic(name = "PublicUntagged")]
#[serde(untagged)]
enum Untagged {
    A(String),
}
#[derive(Schematic)]
#[schematic(name = "PublicId")]
struct Id(u32);
#[derive(Schematic)]
struct Transparent(u32);

#[test]
fn all_derive_shapes_support_names_and_unnamed_newtypes_remain_transparent() {
    let (_, schemas) = collect(|| {
        vec![
            Simple::generate_schema(),
            Internal::generate_schema(),
            Adjacent::generate_schema(),
            External::generate_schema(),
            Untagged::generate_schema(),
            Id::generate_schema(),
            Transparent::generate_schema(),
        ]
    });
    for name in ["PublicEnum", "PublicInternal", "PublicAdjacent", "PublicExternal", "PublicUntagged", "PublicId"] {
        assert!(schemas.contains_key(name), "{name}");
    }
    assert_eq!(schemas.len(), 6);
    assert_eq!(schemas["PublicId"]._type.as_deref(), Some("integer"));
    assert!(serde_json::to_string(&schemas["PublicInternal"])
        .unwrap()
        .contains("#/components/schemas/PublicInternal"));
    assert_eq!(Id::name(), "PublicId");
    assert_eq!(Transparent::name(), "u32");
}

#[test]
fn schema_examples_are_data_even_when_they_look_like_temporary_references() {
    use gotcha_core::registry::{schema_or_ref, SchemaReferences};
    let (_, mut schemas) = collect(|| {
        schema_or_ref("Container", true, || {
            let mut schema = <String as Schematic>::generate_schema();
            schema.schema.extras.insert("example".into(), json!({"$ref": "gotcha:rust-type:Other"}));
            schema
                .schema
                .extras
                .insert("properties".into(), json!({"example": {"$ref": "gotcha:rust-type:Other"}}));
            schema
        })
    });
    let mapping = [("gotcha:rust-type:Other".into(), "#/components/schemas/Resolved".into())]
        .into_iter()
        .collect();
    let schema = schemas.get_mut("Container").unwrap();
    schema.resolve_schema_references(&mapping);
    assert_eq!(schema.extras["example"]["$ref"], "gotcha:rust-type:Other");
    assert_eq!(schema.extras["properties"]["example"]["$ref"], "#/components/schemas/Resolved");
}

mod first {
    pub mod models {
        use super::super::*;
        #[derive(Schematic)]
        pub struct Item {
            value: String,
        }
    }
}
mod second {
    pub mod models {
        use super::super::*;
        #[derive(Schematic)]
        pub struct Item {
            value: u32,
        }
    }
}
#[derive(Schematic)]
struct ScreensSendResult {
    independent: bool,
}
#[derive(Schematic)]
struct Count<const N: usize> {
    value: String,
}
#[derive(Schematic)]
struct Borrowed<'a> {
    value: &'a str,
}
#[derive(Schematic)]
struct WithWhere<T>
where
    T: Schematic,
{
    value: T,
}

#[test]
fn module_prefixes_expand_and_existing_component_names_remain_reserved() {
    let (_, schemas) = collect(|| vec![first::models::Item::generate_schema(), second::models::Item::generate_schema()]);
    assert_eq!(schemas.keys().map(String::as_str).collect::<Vec<_>>(), ["FirstModelsItem", "SecondModelsItem"]);
    let (_, schemas) = collect(|| {
        vec![
            screens::SendResult::generate_schema(),
            terminals::SendResult::generate_schema(),
            ScreensSendResult::generate_schema(),
        ]
    });
    assert_eq!(schemas["ScreensSendResult"].extras["properties"]["independent"]["type"], "boolean");
    assert!(schemas.contains_key("SchemaIdentityScreensSendResult"));
    assert!(schemas.contains_key("TerminalsSendResult"));
}

#[test]
fn const_generics_and_borrowed_types_do_not_require_static_or_lose_their_identity() {
    let (_, schemas) = collect(|| {
        vec![
            Count::<1>::generate_schema(),
            Count::<2>::generate_schema(),
            WithWhere::<String>::generate_schema(),
        ]
    });
    for name in ["Count_1", "Count_2", "WithWhere_String"] {
        assert!(schemas.contains_key(name), "{name}");
    }
    fn borrowed<'a>(_: &'a str) {
        let (_, schemas) = collect(Borrowed::<'a>::generate_schema);
        assert!(
            schemas.contains_key("Borrowed"),
            "{:?}, {}",
            schemas.keys(),
            std::any::type_name::<Borrowed<'a>>()
        );
    }
    borrowed(&String::from("not static"));
}

#[test]
fn colliding_generic_argument_names_keep_distinct_shapes_and_stable_names() {
    let build = |reverse| {
        collect(|| {
            let mut values = vec![
                Envelope::<first::models::Item>::generate_schema(),
                Envelope::<second::models::Item>::generate_schema(),
            ];
            if reverse {
                values.reverse();
            }
            values
        })
    };
    let (_, forward) = build(false);
    let (_, backward) = build(true);
    assert_eq!(serde_json::to_value(&forward).unwrap(), serde_json::to_value(&backward).unwrap());
    assert_eq!(forward.len(), 4);
    let references: Vec<_> = forward
        .iter()
        .filter(|(name, _)| name.contains("Envelope"))
        .map(|(_, schema)| schema.extras["properties"]["value"]["$ref"].as_str().unwrap())
        .collect();
    assert_eq!(references.len(), 2);
    assert_ne!(references[0], references[1]);
}
