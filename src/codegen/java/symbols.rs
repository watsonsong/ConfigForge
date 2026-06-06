// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::validation_error;

pub(super) fn validate_java_package(value: &str) -> Result<()> {
    if value.is_empty()
        || value.split('.').all(|part| {
            !part.is_empty() && is_valid_java_identifier(part) && !is_java_keyword(part)
        })
    {
        Ok(())
    } else {
        Err(validation_error(format!(
            "invalid package `{value}`; expected dot-separated Java identifiers"
        )))
    }
}

pub(super) fn validate_java_identifier(kind: &str, value: &str) -> Result<()> {
    if is_valid_java_identifier(value) && !is_java_keyword(value) {
        Ok(())
    } else {
        Err(validation_error(format!(
            "invalid {kind} `{value}`; expected a non-keyword Java identifier"
        )))
    }
}

fn is_valid_java_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first == '_' || first == '$' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

pub(super) fn is_java_keyword(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "assert"
            | "boolean"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "class"
            | "const"
            | "continue"
            | "default"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "extends"
            | "final"
            | "finally"
            | "float"
            | "for"
            | "goto"
            | "if"
            | "implements"
            | "import"
            | "instanceof"
            | "int"
            | "interface"
            | "long"
            | "native"
            | "new"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "short"
            | "static"
            | "strictfp"
            | "super"
            | "switch"
            | "synchronized"
            | "this"
            | "throw"
            | "throws"
            | "transient"
            | "try"
            | "void"
            | "volatile"
            | "while"
            | "true"
            | "false"
            | "null"
    )
}
