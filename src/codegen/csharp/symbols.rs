// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::validation_error;

pub(super) fn validate_csharp_namespace(value: &str) -> Result<()> {
    if value.split('.').all(|part| {
        !part.is_empty() && is_valid_csharp_identifier(part) && !is_csharp_keyword(part)
    }) {
        Ok(())
    } else {
        Err(validation_error(format!(
            "invalid namespace `{value}`; expected dot-separated C# identifiers"
        )))
    }
}

pub(super) fn validate_csharp_identifier(kind: &str, value: &str) -> Result<()> {
    if is_valid_csharp_identifier(value) && !is_csharp_keyword(value) {
        Ok(())
    } else {
        Err(validation_error(format!(
            "invalid {kind} `{value}`; expected a non-keyword C# identifier"
        )))
    }
}

fn is_valid_csharp_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first == '_' || first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

pub(super) fn is_csharp_keyword(value: &str) -> bool {
    matches!(
        value,
        "abstract"
            | "as"
            | "base"
            | "bool"
            | "break"
            | "byte"
            | "case"
            | "catch"
            | "char"
            | "checked"
            | "class"
            | "const"
            | "continue"
            | "decimal"
            | "default"
            | "delegate"
            | "do"
            | "double"
            | "else"
            | "enum"
            | "event"
            | "explicit"
            | "extern"
            | "false"
            | "finally"
            | "fixed"
            | "float"
            | "for"
            | "foreach"
            | "goto"
            | "if"
            | "implicit"
            | "in"
            | "int"
            | "interface"
            | "internal"
            | "is"
            | "lock"
            | "long"
            | "namespace"
            | "new"
            | "null"
            | "object"
            | "operator"
            | "out"
            | "override"
            | "params"
            | "private"
            | "protected"
            | "public"
            | "readonly"
            | "ref"
            | "return"
            | "sbyte"
            | "sealed"
            | "short"
            | "sizeof"
            | "stackalloc"
            | "static"
            | "string"
            | "struct"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "uint"
            | "ulong"
            | "unchecked"
            | "unsafe"
            | "ushort"
            | "using"
            | "virtual"
            | "void"
            | "volatile"
            | "while"
    )
}
