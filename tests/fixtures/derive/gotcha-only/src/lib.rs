use gotcha as schema;

#[path = "../../schemas.rs"]
pub mod schemas;

#[derive(gotcha::Schematic, gotcha::Validate)]
#[validate(crate = "gotcha::validator")]
pub struct Request {
    #[validate(length(min = 1))]
    #[schematic(example = "Ada")]
    name: String,
}

#[test]
fn validation_uses_gotchas_trait_and_schema_support() {
    assert!(gotcha::Validate::validate(&Request { name: String::new() }).is_err());
    assert!(gotcha::Validate::validate(&Request { name: "Ada".into() }).is_ok());
    let fields = <Request as gotcha::Schematic>::fields();
    assert_eq!(fields[0].1.schema.extras["minLength"], 1);
    assert_eq!(fields[0].1.schema.extras["examples"][0], "Ada");
}

#[test]
fn cargo_info_uses_this_packages_metadata() {
    let info = gotcha::cargo_info!();
    // The macro reads the caller's Cargo metadata, not gotcha's.
    assert_eq!(info.title, "derive-application");
    assert_eq!(info.version, "0.0.0");
    // This fixture declares no description, so the field is omitted rather than empty.
    assert_eq!(info.description, None);
    assert_eq!(info.terms_of_service, None);
    assert!(info.contact.is_none());
    assert!(info.license.is_none());
}
