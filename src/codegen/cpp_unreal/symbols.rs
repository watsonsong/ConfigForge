// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::Result;

use super::types::CppUnrealNames;
use crate::codegen::{CodeModel, GeneratedSymbol, ensure_unique_generated_symbols};

pub(super) fn validate_generated_symbols(
    _model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let mut type_symbols = Vec::new();
    type_symbols.push(GeneratedSymbol::new(
        "root game data type",
        "game_data",
        names.struct_type_name("game_data")?,
    ));
    type_symbols.push(GeneratedSymbol::new(
        "lazy game data type",
        "lazy_game_data",
        "FLazyGameData",
    ));
    for code_enum in &_model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum type",
            &code_enum.name,
            names.enum_type_name(&code_enum.name)?,
        ));
    }
    for code_struct in &_model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct type",
            &code_struct.name,
            names.struct_type_name(&code_struct.name)?,
        ));
    }
    for table in &_model.tables {
        type_symbols.push(GeneratedSymbol::new(
            "row type",
            &table.name,
            names.row_type_name(&table.name)?,
        ));
        if matches!(table.kind, crate::codegen::CodeTableKind::Collection) {
            type_symbols.push(GeneratedSymbol::new(
                "table repository type",
                &table.name,
                names.repository_type_name(&table.name)?,
            ));
        }
    }
    ensure_unique_generated_symbols("cpp_unreal", "top-level types", type_symbols)?;

    for code_struct in &_model.structs {
        let mut field_symbols = Vec::new();
        for field in &code_struct.fields {
            field_symbols.push(GeneratedSymbol::new(
                "field",
                &field.name,
                names.field_name(field)?,
            ));
            if matches!(field.code_type, crate::codegen::CodeTypeRef::Text) {
                field_symbols.push(GeneratedSymbol::new(
                    "text helper",
                    &field.name,
                    names.text_function_name(&field.name)?,
                ));
            }
        }
        field_symbols.push(GeneratedSymbol::new(
            "reader function",
            &code_struct.name,
            names.read_struct_function_name(&code_struct.name)?,
        ));
        ensure_unique_generated_symbols(
            "cpp_unreal",
            &format!("struct `{}`", code_struct.name),
            field_symbols,
        )?;
    }
    for table in &_model.tables {
        let mut field_symbols = Vec::new();
        for field in &table.fields {
            field_symbols.push(GeneratedSymbol::new(
                "field",
                &field.name,
                names.field_name(field)?,
            ));
            if matches!(field.code_type, crate::codegen::CodeTypeRef::Text) {
                field_symbols.push(GeneratedSymbol::new(
                    "text helper",
                    &field.name,
                    names.text_function_name(&field.name)?,
                ));
            }
        }
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            field_symbols.push(GeneratedSymbol::new(
                "relation helper",
                &foreign_key.name,
                names.relation_function_name(foreign_key)?,
            ));
        }
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            field_symbols.push(GeneratedSymbol::new("link function", "link", "Link"));
        }
        field_symbols.push(GeneratedSymbol::new(
            "reader function",
            &table.name,
            names.read_row_function_name(&table.name)?,
        ));
        ensure_unique_generated_symbols(
            "cpp_unreal",
            &format!("table `{}`", table.name),
            field_symbols,
        )?;

        let mut member_symbols = Vec::new();
        member_symbols.push(GeneratedSymbol::new(
            "row storage",
            "Rows",
            names.member_name("rows")?,
        ));
        member_symbols.push(GeneratedSymbol::new(
            "load function",
            "load",
            names.load_function_name()?,
        ));
        member_symbols.push(GeneratedSymbol::new(
            "from rows function",
            "FromRows",
            "FromRows",
        ));
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            member_symbols.push(GeneratedSymbol::new("link function", "link", "Link"));
        }
        for index in &table.indexes {
            member_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
            member_symbols.push(GeneratedSymbol::new(
                "index lookup function",
                &index.name,
                names.index_function_name(index)?,
            ));
            if index.ordered {
                member_symbols.push(GeneratedSymbol::new(
                    "ordered index lower bound function",
                    &index.name,
                    names.lower_bound_index_function_name(index)?,
                ));
                member_symbols.push(GeneratedSymbol::new(
                    "ordered index upper bound function",
                    &index.name,
                    names.upper_bound_index_function_name(index)?,
                ));
                member_symbols.push(GeneratedSymbol::new(
                    "ordered index range function",
                    &index.name,
                    names.range_index_function_name(index)?,
                ));
            }
        }
        ensure_unique_generated_symbols(
            "cpp_unreal",
            &format!("table repository `{}` members", table.name),
            member_symbols,
        )?;
    }
    let mut lazy_symbols = Vec::new();
    lazy_symbols.push(GeneratedSymbol::new(
        "preload all function",
        "PreloadAll",
        "PreloadAll",
    ));
    for table in &_model.tables {
        lazy_symbols.push(GeneratedSymbol::new(
            "lazy table accessor",
            &table.name,
            names.table_accessor_name(&table.name)?,
        ));
        lazy_symbols.push(GeneratedSymbol::new(
            "lazy cache member",
            &table.name,
            names.member_name(&table.name)?,
        ));
    }
    ensure_unique_generated_symbols("cpp_unreal", "FLazyGameData members", lazy_symbols)?;
    Ok(())
}
