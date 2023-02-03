use std::collections::BTreeMap;
use oas::{Operation, Parameter, Responses};
use http::Method;

pub trait Operable {
    fn id(&self) -> &'static str;
    fn method(&self) -> Method;
    fn uri(&self) -> &'static str;
    fn group(&self) -> Option<String>;
    fn description(&self) -> Option<&'static str>;
    fn deprecated(&self) -> bool;
    fn generate(&self) -> Operation {
        let tags = if let Some(group) = self.group() {
            Some(vec![group])
        } else {
            None
        };
        Operation {
            tags,
            summary: None,
            description: self.description().map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id().to_string()),
            parameters: None,
            request_body: None,
            responses: Responses {
                default: None,
                data: BTreeMap::default(),
            },
            callbacks: None,
            deprecated: Some(self.deprecated()),
            security: None,
            servers: None,
        }
    }
}

trait ApiObject {
    fn name() -> &'static str;
    fn required() -> bool;
    fn type_() -> &'static str;
    fn generate() -> Option<Vec<Parameter>>;
}