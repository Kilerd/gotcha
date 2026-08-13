#[cfg(feature = "openapi")]
#[test]
fn test_path_uuid_parameter() {
    use either::Either;
    use gotcha::{ParameterProvider, Path};
    use uuid::Uuid;

    // Test that Path<Uuid> generates a parameter
    let url = "/users/{user_id}".to_string();
    let result = <Path<Uuid> as ParameterProvider>::generate(url);

    match result {
        Either::Left(params) => {
            assert_eq!(params.len(), 1, "Should generate exactly one parameter");
            assert_eq!(params[0].name, "user_id", "Parameter name should be 'user_id'");
            assert_eq!(params[0].required, Some(true), "Path parameter should be required");

            // Verify it's a path parameter
            use oas::ParameterIn;
            assert!(matches!(params[0]._in, ParameterIn::Path), "Should be a path parameter");
        }
        Either::Right(_) => {
            panic!("Path<Uuid> should generate parameters, not a request body");
        }
    }
}

#[cfg(feature = "openapi")]
#[test]
fn test_path_tuple_parameter() {
    use either::Either;
    use gotcha::{ParameterProvider, Path};
    use uuid::Uuid;

    // Test that Path<(Uuid,)> also works (this should already work)
    let url = "/users/{user_id}".to_string();
    let result = <Path<(Uuid,)> as ParameterProvider>::generate(url);

    match result {
        Either::Left(params) => {
            assert_eq!(params.len(), 1, "Should generate exactly one parameter");
            assert_eq!(params[0].name, "user_id", "Parameter name should be 'user_id'");
            assert_eq!(params[0].required, Some(true), "Path parameter should be required");
        }
        Either::Right(_) => {
            panic!("Path<(Uuid,)> should generate parameters, not a request body");
        }
    }
}

#[cfg(feature = "openapi")]
mod path_struct {
    use either::Either;
    use gotcha::{ParameterProvider, Path, Schematic};
    use oas::{Parameter, ParameterIn, Referenceable};

    // Only the derived Schematic impl is exercised; the fields are never read directly.
    #[allow(dead_code)]
    #[derive(Schematic)]
    struct ConnectionPath {
        user_id: String,
        connection_id: String,
    }

    fn assert_per_field_params(params: &[Parameter]) {
        assert_eq!(params.len(), 2, "each struct field should become one parameter");
        for (param, name) in params.iter().zip(["user_id", "connection_id"]) {
            assert_eq!(param.name, name);
            assert!(matches!(param._in, ParameterIn::Path));
            assert_eq!(param.required, Some(true));
            let Some(Referenceable::Data(schema)) = &param.schema else {
                panic!("parameter '{name}' should carry an inline schema");
            };
            assert_eq!(schema._type.as_deref(), Some("string"), "parameter '{name}' should have the field's type");
            assert!(!schema.extras.contains_key("$ref"), "parameter '{name}' must not be a $ref to the whole struct");
        }
    }

    #[test]
    fn struct_fields_become_parameters() {
        let url = "/users/{user_id}/connections/{connection_id}".to_string();
        let result = <Path<ConnectionPath> as ParameterProvider>::generate(url);
        match result {
            Either::Left(params) => assert_per_field_params(&params),
            Either::Right(_) => panic!("Path<ConnectionPath> should generate parameters, not a request body"),
        }
    }

    #[test]
    fn struct_fields_become_parameters_inside_a_collection_scope() {
        // Spec assembly generates every operation inside a collection scope, where a derived
        // struct's generate_schema() returns a bare `$ref`. Parameters must still come out
        // per-field — not one parameter holding the whole object's `$ref`, dropping the rest.
        let url = "/users/{user_id}/connections/{connection_id}".to_string();
        let (result, _schemas) = gotcha_core::registry::collect(|| <Path<ConnectionPath> as ParameterProvider>::generate(url));
        match result {
            Either::Left(params) => assert_per_field_params(&params),
            Either::Right(_) => panic!("Path<ConnectionPath> should generate parameters, not a request body"),
        }
    }
}

#[cfg(feature = "openapi")]
#[test]
fn test_multiple_path_params() {
    use either::Either;
    use gotcha::{ParameterProvider, Path};
    use uuid::Uuid;

    // Test multiple parameters with Path<(Uuid, String)>
    let url = "/users/{user_id}/posts/{post_id}".to_string();
    let result = <Path<(Uuid, String)> as ParameterProvider>::generate(url);

    match result {
        Either::Left(params) => {
            assert_eq!(params.len(), 2, "Should generate two parameters");
            assert_eq!(params[0].name, "user_id", "First parameter name should be 'user_id'");
            assert_eq!(params[1].name, "post_id", "Second parameter name should be 'post_id'");
        }
        Either::Right(_) => {
            panic!("Path<(Uuid, String)> should generate parameters, not a request body");
        }
    }
}
