use http::Method;

#[derive(Debug)]
pub struct GotchaOperationObject {
    summary: String,
}

pub trait Operation {
    fn method(&self) -> Method;
    fn uri(&self) -> &'static str;
    fn summary(&self) -> &'static str;
    // todo description
    fn generate_gotcha_operation_object(&self) -> GotchaOperationObject {
        GotchaOperationObject {
            summary: self.summary().to_string(),
        }
    }
}
