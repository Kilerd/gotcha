use oas::OpenAPIV3;

type Transform = Box<dyn FnOnce(OpenAPIV3) -> OpenAPIV3 + Send>;

/// Ordered transforms for one router: included subtrees first, this router's callbacks last.
/// Keeping the two groups separate makes parent precedence independent of builder-call timing.
#[derive(Default)]
pub(crate) struct OpenApiTransforms {
    children: Vec<Transform>,
    local: Vec<Transform>,
}

impl OpenApiTransforms {
    pub(crate) fn push(&mut self, transform: Transform) {
        self.local.push(transform);
    }

    pub(crate) fn with_child(mut self, child: Self) -> Self {
        self.children.extend(child.children.into_iter().chain(child.local));
        self
    }

    pub(crate) fn apply(self, spec: OpenAPIV3) -> OpenAPIV3 {
        self.children.into_iter().chain(self.local).fold(spec, |spec, transform| transform(spec))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> OpenAPIV3 {
        let mut spec = crate::openapi::generate_openapi(Default::default());
        spec.info.title.clear();
        spec
    }

    fn append(transforms: &mut OpenApiTransforms, name: &str) {
        let name = name.to_owned();
        transforms.push(Box::new(move |mut spec| {
            // Moving the captured value makes this callback FnOnce rather than Fn/FnMut.
            let consumed = name;
            spec.info.title.push_str(&consumed);
            spec
        }));
    }

    #[test]
    fn empty_chain_is_identity_and_local_callbacks_keep_registration_order() {
        let original = spec();
        let expected = serde_json::to_value(&original).unwrap();
        assert_eq!(serde_json::to_value(OpenApiTransforms::default().apply(original)).unwrap(), expected);

        let mut transforms = OpenApiTransforms::default();
        append(&mut transforms, "first ");
        append(&mut transforms, "second");
        assert_eq!(transforms.apply(spec()).info.title, "first second");
    }

    #[test]
    fn subtrees_keep_insertion_order_and_run_before_all_parent_callbacks() {
        let mut parent = OpenApiTransforms::default();
        append(&mut parent, "parent-first ");
        let mut first_child = OpenApiTransforms::default();
        append(&mut first_child, "child-first ");
        let mut grandchild = OpenApiTransforms::default();
        append(&mut grandchild, "grandchild ");
        first_child = first_child.with_child(grandchild);
        append(&mut first_child, "child-second ");
        parent = parent.with_child(first_child);
        append(&mut parent, "parent-second ");
        let mut second_child = OpenApiTransforms::default();
        append(&mut second_child, "sibling ");
        parent = parent.with_child(second_child);
        assert_eq!(
            parent.apply(spec()).info.title,
            "grandchild child-first child-second sibling parent-first parent-second "
        );
    }

    #[test]
    fn child_only_configuration_survives_an_unconfigured_parent() {
        let mut child = OpenApiTransforms::default();
        append(&mut child, "child");
        let parent = OpenApiTransforms::default().with_child(child).with_child(OpenApiTransforms::default());
        assert_eq!(parent.apply(spec()).info.title, "child");
    }

    #[test]
    fn parent_edits_receive_child_metadata_and_can_override_it() {
        let mut child = OpenApiTransforms::default();
        child.push(Box::new(|mut spec| {
            spec.info.title = "Child".into();
            spec.components = Some(
                serde_json::from_value(serde_json::json!({
                    "schemas": {"Shared": {"type": "string"}, "ChildOnly": {"type": "boolean"}}
                }))
                .unwrap(),
            );
            spec.security = Some(vec![oas::SecurityRequirement {
                data: [("child_auth".into(), vec![])].into_iter().collect(),
            }]);
            spec
        }));
        let mut parent = OpenApiTransforms::default();
        parent.push(Box::new(|mut spec| {
            assert_eq!(spec.info.title, "Child");
            assert!(spec.security.as_ref().unwrap()[0].data.contains_key("child_auth"));
            spec.info.title = "Parent".into();
            // Explicit map edits preserve unrelated entries; replacement/merge policy belongs
            // to the callback, not to an implicit framework-level metadata merge.
            spec.components
                .as_mut()
                .unwrap()
                .schemas
                .as_mut()
                .unwrap()
                .insert("Shared".into(), serde_json::from_value(serde_json::json!({"type": "integer"})).unwrap());
            spec.security = Some(vec![]);
            spec
        }));
        let document = serde_json::to_value(parent.with_child(child).apply(spec())).unwrap();
        assert_eq!(document["info"]["title"], "Parent");
        assert_eq!(document["components"]["schemas"]["Shared"]["type"], "integer");
        assert_eq!(document["components"]["schemas"]["ChildOnly"]["type"], "boolean");
        assert_eq!(document["security"], serde_json::json!([]));
    }
}
