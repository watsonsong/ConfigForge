// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{
        CodeFragmentPayload, CodeModel, CodeTableKind, GeneratedSymbol,
        ensure_unique_generated_symbols,
    },
};

use super::types::TypeScriptNames;

pub(super) fn validate_generated_symbols(
    model: &CodeModel,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new("interface", "RuntimeDataLoader", "RuntimeDataLoader"),
        GeneratedSymbol::new("interface", "RuntimeDataReader", "RuntimeDataReader"),
        GeneratedSymbol::new("class", "GameData", "GameData"),
        GeneratedSymbol::new("class", "LazyGameData", "LazyGameData"),
    ];
    for code_enum in &model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum type",
            &code_enum.name,
            names.type_name(&code_enum.name)?,
        ));
    }
    for code_struct in &model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct interface",
            &code_struct.name,
            names.type_name(&code_struct.name)?,
        ));
        validate_named_symbols(
            "typescript",
            &format!("interface `{}` fields", code_struct.name),
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
            "table row interface",
            format!("{}_row", table.name),
            names.row_type_name(&table.name)?,
        ));
        if table.kind == CodeTableKind::Collection {
            type_symbols.push(GeneratedSymbol::new(
                "table class",
                format!("{}_table", table.name),
                names.repository_type_name(&table.name)?,
            ));
            type_symbols.push(GeneratedSymbol::new(
                "row helper",
                format!("{}_row_at_helper", table.name),
                format!("__{}RowAt", names.repository_type_name(&table.name)?),
            ));
            for index in table.indexes.iter().filter(|index| index.unique) {
                type_symbols.push(GeneratedSymbol::new(
                    "index helper",
                    format!("{}_{}_index_helper", table.name, index.name),
                    format!(
                        "__{}{}",
                        names.repository_type_name(&table.name)?,
                        names.index_of_function_name(index)?
                    ),
                ));
            }
        }
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment interface",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "typescript",
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
            "typescript",
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
                }))
                .chain(table.foreign_keys.iter().filter_map(|foreign_key| {
                    foreign_key.linked_field.as_ref().map(|linked_field| {
                        Ok(GeneratedSymbol::new(
                            "relation function",
                            &linked_field.name,
                            names.relation_function_name(foreign_key)?,
                        ))
                    })
                })),
        )?;

        validate_named_symbols(
            "typescript",
            &format!("table `{}` row relation caches", table.name),
            table
                .foreign_keys
                .iter()
                .filter(|foreign_key| foreign_key.linked_field.is_some())
                .map(|foreign_key| {
                    Ok(GeneratedSymbol::new(
                        "relation cache",
                        &foreign_key.name,
                        names.relation_cache_field_name(foreign_key)?,
                    ))
                }),
        )?;

        if table.kind == CodeTableKind::Object {
            continue;
        }

        let mut member_symbols = vec![
            GeneratedSymbol::new("static field", "dataFile", "dataFile"),
            GeneratedSymbol::new("field", "rows_", "rows_"),
        ];
        if table.schema_revision.is_some() {
            member_symbols.push(GeneratedSymbol::new(
                "static field",
                "expectedSchemaRevision",
                "expectedSchemaRevision",
            ));
        }
        for index in &table.indexes {
            member_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "typescript",
            &format!("table `{}` class members", table.name),
            member_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "constructor", "constructor"),
            GeneratedSymbol::new("method", "load", "load"),
            GeneratedSymbol::new("method", "loadSync", "loadSync"),
        ];
        if table.preserve_source_order {
            method_symbols.push(GeneratedSymbol::new("getter", "length", "length"));
            method_symbols.push(GeneratedSymbol::new("method", "rows", "rows"));
            method_symbols.push(GeneratedSymbol::new("method", "rowAt", "rowAt"));
        }
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
        }
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            method_symbols.push(GeneratedSymbol::new("method", "link", "link"));
        }
        ensure_unique_generated_symbols(
            "typescript",
            &format!("table `{}` class methods", table.name),
            method_symbols,
        )?;
    }
    ensure_unique_generated_symbols("typescript", "top-level exports", type_symbols)?;

    validate_named_symbols(
        "typescript",
        "GameData fields",
        model.tables.iter().map(|table| {
            Ok(GeneratedSymbol::new(
                "table field",
                &table.name,
                names.field_name(&table.name)?,
            ))
        }),
    )?;

    let mut game_data_members = vec![
        GeneratedSymbol::new("method", "constructor", "constructor"),
        GeneratedSymbol::new("method", "load", "load"),
        GeneratedSymbol::new("method", "loadSync", "loadSync"),
        GeneratedSymbol::new("method", "loadLazy", "loadLazy"),
        GeneratedSymbol::new("method", "linkAll", "linkAll"),
    ];
    for table in &model.tables {
        game_data_members.push(GeneratedSymbol::new(
            "table field",
            &table.name,
            names.field_name(&table.name)?,
        ));
    }
    ensure_unique_generated_symbols("typescript", "GameData class members", game_data_members)?;

    let mut lazy_game_data_members = vec![
        GeneratedSymbol::new("loader field", "loader", "loader"),
        GeneratedSymbol::new("method", "constructor", "constructor"),
        GeneratedSymbol::new("method", "loadLazy", "loadLazy"),
        GeneratedSymbol::new("method", "preloadAll", "preloadAll"),
        GeneratedSymbol::new("method", "linkAll", "linkAll"),
        GeneratedSymbol::new("method", "preloadAllSync", "preloadAllSync"),
        GeneratedSymbol::new("method", "linkAllSync", "linkAllSync"),
    ];
    for table in &model.tables {
        let field_name = names.field_name(&table.name)?;
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table promise field",
            &table.name,
            format!("{field_name}Promise"),
        ));
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table value field",
            &table.name,
            format!("{field_name}Value"),
        ));
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table loader method",
            &table.name,
            field_name.clone(),
        ));
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table sync loader method",
            &table.name,
            format!("{field_name}Sync"),
        ));
    }
    ensure_unique_generated_symbols(
        "typescript",
        "LazyGameData class members",
        lazy_game_data_members,
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
