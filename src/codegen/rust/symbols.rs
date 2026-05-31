// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{
        CodeFragmentPayload, CodeModel, CodeTableKind, GeneratedSymbol,
        ensure_unique_generated_symbols,
    },
};

use super::types::RustNames;

pub(super) fn validate_generated_symbols(model: &CodeModel, names: &RustNames<'_>) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "runtime interface",
            "RuntimeDataReader",
            "RuntimeDataReader",
        ),
        GeneratedSymbol::new(
            "runtime interface",
            "RuntimeDataLoader",
            "RuntimeDataLoader",
        ),
        GeneratedSymbol::new("runtime interface", "GameDataAccess", "GameDataAccess"),
        GeneratedSymbol::new("facade", "GameData", "GameData"),
        GeneratedSymbol::new("facade", "LazyGameData", "LazyGameData"),
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
            "rust",
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

        validate_named_symbols(
            "rust",
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

        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "rust",
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

        if table.kind == CodeTableKind::Object {
            continue;
        }

        let mut storage_symbols = vec![GeneratedSymbol::new("field", "rows", "rows")];
        for index in &table.indexes {
            storage_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "rust",
            &format!("table `{}` repository fields", table.name),
            storage_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "load", "load"),
            GeneratedSymbol::new("method", "from_rows", "from_rows"),
        ];
        if table.preserve_source_order {
            method_symbols.push(GeneratedSymbol::new("method", "len", "len"));
            method_symbols.push(GeneratedSymbol::new("method", "is_empty", "is_empty"));
            method_symbols.push(GeneratedSymbol::new("method", "rows", "rows"));
            method_symbols.push(GeneratedSymbol::new("method", "row_at", "row_at"));
        }
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "rust",
            &format!("table `{}` methods", table.name),
            method_symbols,
        )?;
    }
    ensure_unique_generated_symbols("rust", "top-level types", type_symbols)?;

    let game_data_fields = model.tables.iter().map(|table| {
        Ok(GeneratedSymbol::new(
            "table field",
            &table.name,
            names.field_name(&table.name)?,
        ))
    });
    validate_named_symbols("rust", "GameData fields", game_data_fields)?;

    let mut lazy_game_data_members = vec![GeneratedSymbol::new("loader field", "loader", "loader")];
    for table in &model.tables {
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table cache field",
            &table.name,
            names.field_name(&table.name)?,
        ));
    }
    validate_named_symbols(
        "rust",
        "LazyGameData fields",
        lazy_game_data_members.into_iter().map(Ok),
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
