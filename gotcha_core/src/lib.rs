use std::collections::BTreeMap;
use oas::{Operation, Parameter, Referenceable, Response, Responses};
use http::Method;
use convert_case::{Case, Casing};

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
            summary: Some(self.id().to_case(Case::Title)),
            description: self.description().map(|v| v.to_string()),
            external_docs: None,
            operation_id: Some(self.id().to_string()),
            parameters: None,
            request_body: None,
            responses: Responses {
                default: Some(Referenceable::Data(Response {
                    description: "default return".to_string(),
                    headers: None,
                    content: None,
                    links: None,
                })),
                data: BTreeMap::default(),
            },
            callbacks: None,
            deprecated: Some(self.deprecated()),
            security: None,
            servers: None,
        }
    }
}

pub trait ApiObject {
    fn name() -> &'static str;
    fn required() -> bool;
    fn type_() -> &'static str;
    fn generate() -> Option<Vec<Parameter>> {
        todo!()
    }
}
