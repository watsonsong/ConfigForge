// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    code_artifact::{
        CodeField, CodeFragmentPayload, CodeModel, CodeTableKind, CodeTypeRef, GeneratedSymbol,
        ensure_unique_generated_symbols,
    },
};

use super::{types::CSharpNames, validation_error};

pub(super) fn validate_generated_symbols(model: &CodeModel, names: &CSharpNames) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "class",
            "ConfigForgeRuntimeException",
            "ConfigForgeRuntimeException",
        ),
        GeneratedSymbol::new("struct", "ConfigForgeColor", "ConfigForgeColor"),
        GeneratedSymbol::new("struct", "ConfigForgeVector2", "ConfigForgeVector2"),
        GeneratedSymbol::new("struct", "ConfigForgeVector3", "ConfigForgeVector3"),
        GeneratedSymbol::new("struct", "ConfigForgeVector4", "ConfigForgeVector4"),
        GeneratedSymbol::new("struct", "ConfigForgeVector2Int", "ConfigForgeVector2Int"),
        GeneratedSymbol::new("struct", "ConfigForgeVector3Int", "ConfigForgeVector3Int"),
        GeneratedSymbol::new("struct", "ConfigForgeVector4Int", "ConfigForgeVector4Int"),
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
            &format!("struct `{}` properties", code_struct.name),
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
                        "table `{}` fragment `{}` properties",
                        table.name, fragment.name
                    ),
                    &fragment.fields,
                    names,
                )?;
            }
        }
        validate_named_symbols(
            &format!("table `{}` row properties", table.name),
            table
                .fields
                .iter()
                .map(|field| {
                    let mut symbols = vec![GeneratedSymbol::new(
                        "field",
                        &field.name,
                        names.property_name(&field.name)?,
                    )];
                    if matches!(field.code_type, CodeTypeRef::Text) {
                        symbols.push(GeneratedSymbol::new(
                            "text helper",
                            &field.name,
                            names.text_function_name(&field.name)?,
                        ));
                    }
                    Ok(symbols)
                })
                .chain(table.fragments.iter().map(|fragment| {
                    Ok(vec![GeneratedSymbol::new(
                        "fragment field",
                        &fragment.name,
                        names.property_name(&fragment.name)?,
                    )])
                }))
                .chain(
                    table
                        .indexes
                        .iter()
                        .filter(|index| !index.ordered)
                        .map(|index| {
                            Ok(vec![GeneratedSymbol::new(
                                "index method",
                                &index.name,
                                names.index_function_name(index)?,
                            )])
                        }),
                )
                .chain(table.foreign_keys.iter().filter_map(|foreign_key| {
                    foreign_key.linked_field.as_ref()?;
                    Some(names.relation_function_name(foreign_key).map(|generated| {
                        vec![GeneratedSymbol::new(
                            "relation helper",
                            &foreign_key.name,
                            generated,
                        )]
                    }))
                })),
        )?;
    }

    ensure_unique_generated_symbols("csharp", "top-level types", type_symbols)
}

fn validate_field_symbols(scope: &str, fields: &[CodeField], names: &CSharpNames) -> Result<()> {
    validate_named_symbols(
        scope,
        fields.iter().map(|field| {
            let mut symbols = vec![GeneratedSymbol::new(
                "field",
                &field.name,
                names.property_name(&field.name)?,
            )];
            if matches!(field.code_type, CodeTypeRef::Text) {
                symbols.push(GeneratedSymbol::new(
                    "text helper",
                    &field.name,
                    names.text_function_name(&field.name)?,
                ));
            }
            Ok(symbols)
        }),
    )
}

fn validate_named_symbols(
    scope: &str,
    symbols: impl IntoIterator<Item = Result<Vec<GeneratedSymbol>>>,
) -> Result<()> {
    ensure_unique_generated_symbols(
        "csharp",
        scope,
        symbols
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect(),
    )
}

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
