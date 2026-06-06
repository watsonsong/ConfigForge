// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{
        CodeField, CodeFragmentPayload, CodeModel, CodeTableKind, GeneratedSymbol,
        ensure_unique_generated_symbols,
    },
};

use super::{types::JavaNames, validation_error};

pub(super) fn validate_generated_symbols(model: &CodeModel, names: &JavaNames) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "class",
            "ConfigForgeRuntimeException",
            "ConfigForgeRuntimeException",
        ),
        GeneratedSymbol::new(
            "interface",
            "ConfigForgeRuntimeDataReader",
            "ConfigForgeRuntimeDataReader",
        ),
        GeneratedSymbol::new(
            "interface",
            "ConfigForgeRuntimeDataLoader",
            "ConfigForgeRuntimeDataLoader",
        ),
        GeneratedSymbol::new("interface", "GameDataAccess", "GameDataAccess"),
        GeneratedSymbol::new("class", "GameData", names.game_data_class_name()),
        GeneratedSymbol::new("class", "LazyGameData", names.lazy_game_data_class_name()),
    ];

    for code_enum in &model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum type",
            &code_enum.name,
            names.type_name(&code_enum.name)?,
        ));
        validate_named_symbols(
            &format!("enum `{}` values", code_enum.name),
            code_enum.values.iter().map(|value| {
                Ok(vec![GeneratedSymbol::new(
                    "enum value",
                    &value.name,
                    names.enum_value_name(&value.name)?,
                )])
            }),
        )?;
    }

    for code_struct in &model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct class",
            &code_struct.name,
            names.type_name(&code_struct.name)?,
        ));
        validate_field_symbols(
            &format!("struct `{}` members", code_struct.name),
            &code_struct.fields,
            names,
        )?;
    }

    for table in &model.tables {
        type_symbols.push(GeneratedSymbol::new(
            "table row class",
            format!("{}_row", table.name),
            names.row_type_name(&table.name)?,
        ));
        if table.kind == CodeTableKind::Collection {
            type_symbols.push(GeneratedSymbol::new(
                "table class",
                format!("{}_table", table.name),
                names.repository_type_name(&table.name)?,
            ));
        }
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment class",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_field_symbols(
                    &format!(
                        "table `{}` fragment `{}` members",
                        table.name, fragment.name
                    ),
                    &fragment.fields,
                    names,
                )?;
            }
        }
        validate_named_symbols(
            &format!("table `{}` row members", table.name),
            table
                .fields
                .iter()
                .map(|field| field_symbols(field, names))
                .chain(table.fragments.iter().map(|fragment| {
                    Ok(vec![
                        GeneratedSymbol::new(
                            "fragment field",
                            &fragment.name,
                            names.field_name(&fragment.name)?,
                        ),
                        GeneratedSymbol::new(
                            "fragment getter",
                            &fragment.name,
                            names.getter_name(&fragment.name)?,
                        ),
                    ])
                })),
        )?;
    }

    ensure_unique_generated_symbols("java", "nested types", type_symbols)
}

fn validate_field_symbols(scope: &str, fields: &[CodeField], names: &JavaNames) -> Result<()> {
    validate_named_symbols(
        scope,
        fields.iter().map(|field| field_symbols(field, names)),
    )
}

fn field_symbols(field: &CodeField, names: &JavaNames) -> Result<Vec<GeneratedSymbol>> {
    Ok(vec![
        GeneratedSymbol::new("field", &field.name, names.field_name(&field.name)?),
        GeneratedSymbol::new("getter", &field.name, names.getter_name(&field.name)?),
    ])
}

fn validate_named_symbols(
    scope: &str,
    symbols: impl IntoIterator<Item = Result<Vec<GeneratedSymbol>>>,
) -> Result<()> {
    ensure_unique_generated_symbols(
        "java",
        scope,
        symbols
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect(),
    )
}

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
