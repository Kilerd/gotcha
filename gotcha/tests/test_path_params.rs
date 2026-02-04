#[cfg(feature = "openapi")]
#[test]
fn test_path_uuid_parameter() {
    use either::Either;
    use gotcha::{ParameterProvider, Path};
    use uuid::Uuid;

    // Test that Path<Uuid> generates a parameter
    let url = "/users/:user_id".to_string();
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
    let url = "/users/:user_id".to_string();
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
#[test]
fn test_multiple_path_params() {
    use either::Either;
    use gotcha::{ParameterProvider, Path};
    use uuid::Uuid;

    // Test multiple parameters with Path<(Uuid, String)>
    let url = "/users/:user_id/posts/:post_id".to_string();
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