use syn::{Attribute, Lit, Meta};

pub trait AttributesExt {
    fn get_doc(&self) -> Option<String>;
}

impl AttributesExt for Vec<Attribute> {
    fn get_doc(&self) -> Option<String> {
        let docs: Vec<String> = self
            .iter()
            .filter_map(|attr| match attr.parse_meta().unwrap() {
                Meta::NameValue(doc) => {
                    if doc.path.is_ident("doc") {
                        Some(doc)
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .filter_map(|attr| match attr.lit {
                Lit::Str(lit_str) => Some(lit_str.value()),
                _ => None,
            })
            .map(|doc| doc.trim().to_string())
            .collect();
        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }
}
