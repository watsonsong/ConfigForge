// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use super::{CaseStyle, KeywordConflictPolicy};

pub fn pascal_case(value: &str) -> String {
    let mut output = value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();
    if output.is_empty() {
        output.push_str("Generated");
    }
    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        output.insert_str(0, "Generated");
    }
    output
}

pub fn snake_case(value: &str) -> String {
    let words = words(value);
    let mut output = if words.is_empty() {
        "field".to_owned()
    } else {
        words.join("_")
    };
    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        output.insert_str(0, "field_");
    }
    output
}

pub fn camel_case(value: &str) -> String {
    let words = words(value);
    if words.is_empty() {
        return "field".to_owned();
    }

    let mut output = words[0].clone();
    for word in words.iter().skip(1) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.push_str(chars.as_str());
        }
    }
    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        output.insert_str(0, "field");
    }
    output
}

pub fn apply_case_style(value: &str, style: CaseStyle) -> String {
    match style {
        CaseStyle::Preserve => preserve_identifier(value),
        CaseStyle::PascalCase => pascal_case(value),
        CaseStyle::CamelCase => camel_case(value),
        CaseStyle::SnakeCase => snake_case(value),
        CaseStyle::ScreamingSnakeCase => snake_case(value).to_ascii_uppercase(),
    }
}

pub fn resolve_keyword_conflict(
    identifier: String,
    policy: KeywordConflictPolicy,
    is_keyword: impl Fn(&str) -> bool,
    raw_identifier: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    if !is_keyword(&identifier) {
        return Ok(identifier);
    }

    match policy {
        KeywordConflictPolicy::SuffixUnderscore => Ok(format!("{identifier}_")),
        KeywordConflictPolicy::RawIdentifier => raw_identifier(&identifier)
            .ok_or_else(|| format!("raw identifier is not supported for `{identifier}`")),
        KeywordConflictPolicy::Error => Err(format!("identifier `{identifier}` is a keyword")),
    }
}

pub fn make_identifier(
    value: &str,
    style: CaseStyle,
    policy: KeywordConflictPolicy,
    is_keyword: impl Fn(&str) -> bool,
    raw_identifier: impl Fn(&str) -> Option<String>,
) -> Result<String, String> {
    resolve_keyword_conflict(
        apply_case_style(value, style),
        policy,
        is_keyword,
        raw_identifier,
    )
}

fn words(value: &str) -> Vec<String> {
    value
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

fn preserve_identifier(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if output.is_empty() {
        output.push_str("field");
    }
    if output.starts_with(|ch: char| ch.is_ascii_digit()) {
        output.insert_str(0, "field_");
    }
    output
}

pub fn is_rust_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "async"
            | "await"
            | "become"
            | "box"
            | "do"
            | "dyn"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

pub fn is_cpp_keyword(value: &str) -> bool {
    matches!(
        value,
        "alignas"
            | "alignof"
            | "and"
            | "asm"
            | "auto"
            | "bool"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "constexpr"
            | "continue"
            | "default"
            | "delete"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "explicit"
            | "export"
            | "extern"
            | "false"
            | "float"
            | "for"
            | "friend"
            | "if"
            | "inline"
            | "int"
            | "long"
            | "namespace"
            | "new"
            | "operator"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "signed"
            | "sizeof"
            | "static"
            | "struct"
            | "switch"
            | "template"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typedef"
            | "typename"
            | "union"
            | "unsigned"
            | "using"
            | "virtual"
            | "void"
            | "while"
    )
}

pub fn is_ts_keyword(value: &str) -> bool {
    matches!(
        value,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}
