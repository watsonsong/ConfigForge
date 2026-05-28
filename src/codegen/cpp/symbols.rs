// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{codegen::{ensure_unique_generated_symbols, CodeFragmentPayload, CodeModel, CodeTableKind, GeneratedSymbol}, Result};

use super::types::CppNames;

pub(super) fn validate_generated_symbols(model: &CodeModel, names: &CppNames<'_>) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "runtime interface",
            "IRuntimeDataReader",
            "IRuntimeDataReader",
        ),
        GeneratedSymbol::new(
            "runtime interface",
            "IRuntimeDataLoader",
            "IRuntimeDataLoader",
        ),
    ];
    for code_enum in &model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum",
            &code_enum.name,
            names.type_name(&code_enum.name)?,
        ));
    }
    for code_struct in &model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct",
            &code_struct.name,
            names.type_name(&code_struct.name)?,
        ));
        validate_named_symbols(
            "cpp",
            &format!("struct `{}` fields", code_struct.name),
            code_struct.fields.iter().map(|field| {
                Ok(GeneratedSymbol::new(
                    "field",
                    &field.name,
                    names.field_name(&field.name)?,
                ))
            }),
        )?;
    }
    for table in &model.tables {
        type_symbols.push(GeneratedSymbol::new(
            "table row",
            format!("{}_row", table.name),
            names.row_type_name(&table.name)?,
        ));
        if table.kind == CodeTableKind::Collection {
            type_symbols.push(GeneratedSymbol::new(
                "table repository",
                format!("{}_table", table.name),
                names.repository_type_name(&table.name)?,
            ));
        }
        for index in &table.indexes {
            if index.fields.len() > 1 {
                type_symbols.push(GeneratedSymbol::new(
                    "compound index key",
                    format!("{}_{}_key", table.name, index.name),
                    names.compound_index_key_type_name(table, index)?,
                ));
            }
        }
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "cpp",
                    &format!("table `{}` fragment `{}` fields", table.name, fragment.name),
                    fragment.fields.iter().map(|field| {
                        Ok(GeneratedSymbol::new(
                            "field",
                            &field.name,
                            names.field_name(&field.name)?,
                        ))
                    }),
                )?;
            }
        }

        validate_named_symbols(
            "cpp",
            &format!("table `{}` row fields", table.name),
            table
                .fields
                .iter()
                .map(|field| {
                    Ok(GeneratedSymbol::new(
                        "field",
                        &field.name,
                        names.field_name(&field.name)?,
                    ))
                })
                .chain(table.fragments.iter().map(|fragment| {
                    Ok(GeneratedSymbol::new(
                        "fragment field",
                        &fragment.name,
                        names.field_name(&fragment.name)?,
                    ))
                })),
        )?;

        if table.kind == CodeTableKind::Object {
            continue;
        }

        let mut member_symbols = vec![GeneratedSymbol::new("field", "rows_", "rows_")];
        for index in &table.indexes {
            member_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "cpp",
            &format!("table `{}` private members", table.name),
            member_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "Load", "Load"),
            GeneratedSymbol::new("method", "FromRows", "FromRows"),
            GeneratedSymbol::new("method", "Rows", "Rows"),
        ];
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "cpp",
            &format!("table `{}` methods", table.name),
            method_symbols,
        )?;
    }
    type_symbols.push(GeneratedSymbol::new("class", "GameData", "GameData"));
    ensure_unique_generated_symbols("cpp", "top-level types", type_symbols)?;

    validate_named_symbols(
        "cpp",
        "GameData members",
        model.tables.iter().map(|table| {
            Ok(GeneratedSymbol::new(
                "table member",
                &table.name,
                names.member_name(&table.name)?,
            ))
        }),
    )?;

    Ok(())
}

fn validate_named_symbols(
    language: &str,
    scope: &str,
    symbols: impl Iterator<Item = Result<GeneratedSymbol>>,
) -> Result<()> {
    ensure_unique_generated_symbols(language, scope, symbols.collect::<Result<Vec<_>>>()?)
}
