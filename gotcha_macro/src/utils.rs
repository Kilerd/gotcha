use syn::{Attribute, Lit, Meta};

pub trait AttributesExt {
    fn get_doc(&self) -> Option<String>;
}

/// Serde rename_all case conventions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameAll {
    /// lowercase
    LowerCase,
    /// UPPERCASE
    UpperCase,
    /// PascalCase
    PascalCase,
    /// camelCase
    CamelCase,
    /// snake_case
    SnakeCase,
    /// SCREAMING_SNAKE_CASE
    ScreamingSnakeCase,
    /// kebab-case
    KebabCase,
    /// SCREAMING-KEBAB-CASE
    ScreamingKebabCase,
}

impl RenameAll {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "lowercase" => Some(RenameAll::LowerCase),
            "UPPERCASE" => Some(RenameAll::UpperCase),
            "PascalCase" => Some(RenameAll::PascalCase),
            "camelCase" => Some(RenameAll::CamelCase),
            "snake_case" => Some(RenameAll::SnakeCase),
            "SCREAMING_SNAKE_CASE" => Some(RenameAll::ScreamingSnakeCase),
            "kebab-case" => Some(RenameAll::KebabCase),
            "SCREAMING-KEBAB-CASE" => Some(RenameAll::ScreamingKebabCase),
            _ => None,
        }
    }

    /// Apply rename_all transformation to any identifier.
    /// First normalizes to words, then converts to target case.
    pub fn apply(&self, name: &str) -> String {
        let words = split_into_words(name);
        match self {
            RenameAll::LowerCase => words.join("").to_lowercase(),
            RenameAll::UpperCase => words.join("").to_uppercase(),
            RenameAll::PascalCase => words_to_pascal_case(&words),
            RenameAll::CamelCase => words_to_camel_case(&words),
            RenameAll::SnakeCase => words_to_snake_case(&words),
            RenameAll::ScreamingSnakeCase => words_to_snake_case(&words).to_uppercase(),
            RenameAll::KebabCase => words_to_kebab_case(&words),
            RenameAll::ScreamingKebabCase => words_to_kebab_case(&words).to_uppercase(),
        }
    }
}

/// Split an identifier into words.
/// Handles both PascalCase/camelCase and snake_case/kebab-case inputs.
fn split_into_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();

    // First, split by underscores and hyphens
    for part in s.split(|c| c == '_' || c == '-') {
        if part.is_empty() {
            continue;
        }

        // Then split camelCase/PascalCase within each part
        let mut prev_was_upper = false;
        for (i, c) in part.chars().enumerate() {
            if c.is_uppercase() {
                if !current_word.is_empty() {
                    // Check if this is start of new word or continuation of acronym
                    let next_is_lower = part.chars().nth(i + 1).map(|n| n.is_lowercase()).unwrap_or(false);
                    if !prev_was_upper || next_is_lower {
                        words.push(current_word.to_lowercase());
                        current_word = String::new();
                    }
                }
                current_word.push(c);
                prev_was_upper = true;
            } else {
                current_word.push(c);
                prev_was_upper = false;
            }
        }
        if !current_word.is_empty() {
            words.push(current_word.to_lowercase());
            current_word = String::new();
        }
    }

    words
}

fn words_to_pascal_case(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn words_to_camel_case(words: &[String]) -> String {
    let mut result = String::new();
    for (i, word) in words.iter().enumerate() {
        if i == 0 {
            result.push_str(&word.to_lowercase());
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap());
                result.extend(chars);
            }
        }
    }
    result
}

fn words_to_snake_case(words: &[String]) -> String {
    words.join("_")
}

fn words_to_kebab_case(words: &[String]) -> String {
    words.join("-")
}

/// Parse serde rename attribute from field/variant attributes
pub fn parse_serde_rename(attrs: &[Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path.is_ident("serde") {
            if let Ok(nested) = attr.parse_args_with(
                |input: syn::parse::ParseStream| {
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input)
                },
            ) {
                for meta in nested {
                    if let syn::Meta::NameValue(name_value) = meta {
                        if name_value.path.is_ident("rename") {
                            if let syn::Lit::Str(lit_str) = name_value.lit {
                                return Some(lit_str.value());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse serde rename_all attribute from container attributes
pub fn parse_serde_rename_all(attrs: &[Attribute]) -> Option<RenameAll> {
    for attr in attrs {
        if attr.path.is_ident("serde") {
            if let Ok(nested) = attr.parse_args_with(
                |input: syn::parse::ParseStream| {
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated(input)
                },
            ) {
                for meta in nested {
                    if let syn::Meta::NameValue(name_value) = meta {
                        if name_value.path.is_ident("rename_all") {
                            if let syn::Lit::Str(lit_str) = name_value.lit {
                                return RenameAll::from_str(&lit_str.value());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Get the effective serialized name for a field/variant
pub fn get_serde_name(ident: &str, rename: Option<&str>, rename_all: Option<RenameAll>) -> String {
    if let Some(renamed) = rename {
        renamed.to_string()
    } else if let Some(case) = rename_all {
        case.apply(ident)
    } else {
        ident.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_into_words() {
        // PascalCase
        assert_eq!(split_into_words("InProgress"), vec!["in", "progress"]);
        assert_eq!(split_into_words("HTTPServer"), vec!["http", "server"]);
        assert_eq!(split_into_words("Simple"), vec!["simple"]);

        // snake_case
        assert_eq!(split_into_words("user_name"), vec!["user", "name"]);
        assert_eq!(split_into_words("created_at"), vec!["created", "at"]);

        // kebab-case
        assert_eq!(split_into_words("user-name"), vec!["user", "name"]);

        // Mixed
        assert_eq!(split_into_words("XMLParser"), vec!["xml", "parser"]);
    }

    #[test]
    fn test_rename_all_apply_pascal_case_input() {
        // Enum variants are typically PascalCase
        assert_eq!(RenameAll::KebabCase.apply("InProgress"), "in-progress");
        assert_eq!(RenameAll::SnakeCase.apply("InProgress"), "in_progress");
        assert_eq!(RenameAll::CamelCase.apply("InProgress"), "inProgress");
        assert_eq!(RenameAll::LowerCase.apply("InProgress"), "inprogress");
        assert_eq!(RenameAll::UpperCase.apply("InProgress"), "INPROGRESS");
        assert_eq!(RenameAll::ScreamingSnakeCase.apply("InProgress"), "IN_PROGRESS");
        assert_eq!(RenameAll::ScreamingKebabCase.apply("InProgress"), "IN-PROGRESS");
        assert_eq!(RenameAll::PascalCase.apply("InProgress"), "InProgress");
    }

    #[test]
    fn test_rename_all_apply_snake_case_input() {
        // Struct fields are typically snake_case
        assert_eq!(RenameAll::CamelCase.apply("user_name"), "userName");
        assert_eq!(RenameAll::PascalCase.apply("user_name"), "UserName");
        assert_eq!(RenameAll::KebabCase.apply("user_name"), "user-name");
        assert_eq!(RenameAll::SnakeCase.apply("user_name"), "user_name");
        assert_eq!(RenameAll::ScreamingSnakeCase.apply("max_retry_count"), "MAX_RETRY_COUNT");
    }

    #[test]
    fn test_get_serde_name() {
        // rename takes precedence
        assert_eq!(
            get_serde_name("InProgress", Some("custom"), Some(RenameAll::KebabCase)),
            "custom"
        );
        // rename_all applied when no rename
        assert_eq!(
            get_serde_name("InProgress", None, Some(RenameAll::KebabCase)),
            "in-progress"
        );
        // original name when neither
        assert_eq!(get_serde_name("InProgress", None, None), "InProgress");
    }
}

impl AttributesExt for Vec<Attribute> {
    fn get_doc(&self) -> Option<String> {
        let docs: Vec<String> = self
            .iter()
            .filter_map(|attr| match attr.parse_meta().expect("Failed to parse attribute to get doc") {
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
