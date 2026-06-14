// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    code_artifact::{
        CodeFragmentPayload, CodeModel, CodeTableKind, GeneratedSymbol,
        ensure_unique_generated_symbols,
    },
};

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
        GeneratedSymbol::new("facade interface", "GameDataAccess", "GameDataAccess"),
        GeneratedSymbol::new("builtin struct", "ConfigForgeColor", "ConfigForgeColor"),
        GeneratedSymbol::new("builtin struct", "ConfigForgeVector2", "ConfigForgeVector2"),
        GeneratedSymbol::new("builtin struct", "ConfigForgeVector3", "ConfigForgeVector3"),
        GeneratedSymbol::new("builtin struct", "ConfigForgeVector4", "ConfigForgeVector4"),
        GeneratedSymbol::new(
            "builtin struct",
            "ConfigForgeVector2Int",
            "ConfigForgeVector2Int",
        ),
        GeneratedSymbol::new(
            "builtin struct",
            "ConfigForgeVector3Int",
            "ConfigForgeVector3Int",
        ),
        GeneratedSymbol::new(
            "builtin struct",
            "ConfigForgeVector4Int",
            "ConfigForgeVector4Int",
        ),
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
                type_symbols.push(GeneratedSymbol::new(
                    "compound index key hash",
                    format!("{}_{}_key_hash", table.name, index.name),
                    names.compound_index_key_hash_type_name(table, index)?,
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
            "cpp",
            &format!("table `{}` row cache members", table.name),
            table
                .foreign_keys
                .iter()
                .filter(|foreign_key| foreign_key.linked_field.is_some())
                .map(|foreign_key| {
                    Ok(GeneratedSymbol::new(
                        "relation cache",
                        &foreign_key.name,
                        names.relation_cache_member_name(foreign_key)?,
                    ))
                }),
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
            GeneratedSymbol::new("private method", "RowByHandle", "RowByHandle"),
        ];
        if table.preserve_source_order {
            method_symbols.push(GeneratedSymbol::new("method", "Size", "Size"));
            method_symbols.push(GeneratedSymbol::new("method", "Rows", "Rows"));
            method_symbols.push(GeneratedSymbol::new("method", "RowAt", "RowAt"));
        }
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
            if index.ordered {
                method_symbols.push(GeneratedSymbol::new(
                    "ordered index lower bound function",
                    &index.name,
                    names.lower_bound_index_function_name(index)?,
                ));
                method_symbols.push(GeneratedSymbol::new(
                    "ordered index upper bound function",
                    &index.name,
                    names.upper_bound_index_function_name(index)?,
                ));
                method_symbols.push(GeneratedSymbol::new(
                    "ordered index range function",
                    &index.name,
                    names.range_index_function_name(index)?,
                ));
            }
        }
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            method_symbols.push(GeneratedSymbol::new("method", "Link", "Link"));
        }
        ensure_unique_generated_symbols(
            "cpp",
            &format!("table `{}` methods", table.name),
            method_symbols,
        )?;
    }
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

    let mut lazy_game_data_members =
        vec![GeneratedSymbol::new("loader member", "loader", "loader_")];
    for table in &model.tables {
        lazy_game_data_members.push(GeneratedSymbol::new(
            "table cache member",
            &table.name,
            names.member_name(&table.name)?,
        ));
    }
    validate_named_symbols(
        "cpp",
        "LazyGameData members",
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
