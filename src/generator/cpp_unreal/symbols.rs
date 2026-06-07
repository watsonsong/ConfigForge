// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    code_artifact::{
        CodeField, CodeIndex, CodeTable, CodeTableKind, CodeTypeRef, CodegenOptions,
        ir::CodeForeignKey, naming,
    },
};

use super::CppUnrealCodegenOptions;

pub(super) struct CppUnrealNames<'a> {
    options: &'a CodegenOptions,
    unreal_options: &'a CppUnrealCodegenOptions,
}

impl<'a> CppUnrealNames<'a> {
    pub(super) fn new(
        options: &'a CodegenOptions,
        unreal_options: &'a CppUnrealCodegenOptions,
    ) -> Self {
        Self {
            options,
            unreal_options,
        }
    }

    pub(super) fn type_name(&self, name: &str) -> Result<String> {
        Ok(format!(
            "{}{}",
            self.unreal_options
                .type_name_prefix
                .as_deref()
                .unwrap_or(""),
            self.identifier(name, self.options.naming.type_name)?
        ))
    }

    pub(super) fn enum_type_name(&self, name: &str) -> Result<String> {
        Ok(format!("E{}", self.type_name(name)?))
    }

    pub(super) fn struct_type_name(&self, name: &str) -> Result<String> {
        Ok(format!("F{}", self.type_name(name)?))
    }

    pub(super) fn lazy_game_data_type_name(&self) -> Result<String> {
        self.struct_type_name("lazy_game_data")
    }

    pub(super) fn game_data_access_interface_name(&self) -> Result<String> {
        Ok(format!(
            "I{}GameDataAccess",
            self.type_name("config_forge")?
        ))
    }

    pub(super) fn row_type_name(&self, table_name: &str) -> Result<String> {
        Ok(format!("F{}Row", self.type_name(table_name)?))
    }

    pub(super) fn repository_type_name(&self, table_name: &str) -> Result<String> {
        Ok(format!("F{}Table", self.type_name(table_name)?))
    }

    pub(super) fn table_data_type_name(&self, table: &CodeTable) -> Result<String> {
        match table.kind {
            CodeTableKind::Object => self.row_type_name(&table.name),
            CodeTableKind::Collection => self.repository_type_name(&table.name),
        }
    }

    pub(super) fn read_row_function_name(&self, table_name: &str) -> Result<String> {
        self.identifier(
            &format!("read_{table_name}_row"),
            self.options.naming.function,
        )
    }

    pub(super) fn read_struct_function_name(&self, struct_name: &str) -> Result<String> {
        self.identifier(&format!("read_{struct_name}"), self.options.naming.function)
    }

    pub(super) fn load_function_name(&self) -> Result<String> {
        self.identifier("load", self.options.naming.function)
    }

    pub(super) fn data_file_constant_name(&self) -> Result<String> {
        self.identifier("data_file", self.options.naming.constant)
    }

    pub(super) fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    pub(super) fn field_name(&self, field: &CodeField) -> Result<String> {
        let name = self.identifier(&field.name, self.options.naming.field)?;
        if matches!(field.code_type, CodeTypeRef::Bool) && !name.starts_with('b') {
            Ok(format!("b{name}"))
        } else {
            Ok(name)
        }
    }

    pub(super) fn member_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    pub(super) fn table_accessor_name(&self, table_name: &str) -> Result<String> {
        self.identifier(&format!("get_{table_name}"), self.options.naming.function)
    }

    pub(super) fn text_function_name(&self, field_name: &str) -> Result<String> {
        self.identifier(&format!("{field_name}_text"), self.options.naming.function)
    }

    pub(super) fn relation_function_name(&self, foreign_key: &CodeForeignKey) -> Result<String> {
        let name = foreign_key
            .linked_field
            .as_ref()
            .map(|linked_field| linked_field.name.as_str())
            .unwrap_or(foreign_key.name.as_str());
        self.identifier(&format!("get_{name}"), self.options.naming.function)
    }

    pub(super) fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.identifier(&index.name, self.options.naming.field)
    }

    pub(super) fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    pub(super) fn lower_bound_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("lower_bound_by_{suffix}"),
            self.options.naming.function,
        )
    }

    pub(super) fn upper_bound_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("upper_bound_by_{suffix}"),
            self.options.naming.function,
        )
    }

    pub(super) fn range_index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("range_by_{suffix}"), self.options.naming.function)
    }

    pub(super) fn blueprint_library_type_name(&self) -> Result<String> {
        Ok(format!(
            "U{}BlueprintLibrary",
            self.type_name("config_forge")?
        ))
    }

    pub(super) fn blueprint_index_function_name(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        let prefix = if index.unique { "get" } else { "find" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("{prefix}_{}_by_{suffix}", table.name),
            self.options.naming.function,
        )
    }

    pub(super) fn blueprint_range_index_function_name(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(
            &format!("range_{}_by_{suffix}", table.name),
            self.options.naming.function,
        )
    }

    pub(super) fn cpp_index_params(&self, index: &CodeIndex) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.cpp_field_type(field)?,
                    self.field_name(field)?
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|params| params.join(", "))
    }

    pub(super) fn cpp_range_index_params(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        let key_type = self.cpp_key_type(table, index)?;
        Ok(format!("{key_type} Lower, {key_type} Upper"))
    }

    pub(super) fn cpp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "int64".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "FString".to_owned(),
            CodeTypeRef::Enum { name } => self.enum_type_name(name)?,
            CodeTypeRef::Struct { name } => self.struct_type_name(name)?,
            CodeTypeRef::List(inner) => format!("TArray<{}>", self.cpp_type(inner)?),
        })
    }

    pub(super) fn cpp_field_type(&self, field: &CodeField) -> Result<String> {
        let inner = self.cpp_type(&field.code_type)?;
        if field.required {
            Ok(inner)
        } else {
            Ok(format!("TOptional<{inner}>"))
        }
    }

    pub(super) fn cpp_index_storage(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        let key_type = self.cpp_key_type(table, index)?;
        if index.ordered {
            return Ok(format!("TArray<TPair<{key_type}, int32>>"));
        }
        if index.unique {
            Ok(format!("TMap<{key_type}, int32>"))
        } else {
            Ok(format!("TMap<{key_type}, TArray<int32>>"))
        }
    }

    pub(super) fn cpp_key_type(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.cpp_field_type(&index.fields[0])
        } else {
            self.compound_index_key_type_name(table, index)
        }
    }

    pub(super) fn cpp_row_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        row_expr: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "{}.{}",
                row_expr,
                self.field_name(&index.fields[0])?
            ))
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{}.{}", row_expr, self.field_name(field)?))),
            )
        }
    }

    pub(super) fn cpp_lookup_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0])
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index.fields.iter().map(|field| self.field_name(field)),
            )
        }
    }

    fn cpp_compound_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        values: impl Iterator<Item = Result<String>>,
    ) -> Result<String> {
        Ok(format!(
            "{}{{{}}}",
            self.compound_index_key_type_name(table, index)?,
            values.collect::<Result<Vec<_>>>()?.join(", ")
        ))
    }

    pub(super) fn compound_index_key_type_name(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
    ) -> Result<String> {
        Ok(format!(
            "F{}{}Key",
            self.type_name(&table.name)?,
            self.type_name(&index.name)?
        ))
    }

    fn identifier(&self, value: &str, style: crate::code_artifact::CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_cpp_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("cpp_unreal codegen: {message}")))
    }
}

use crate::code_artifact::{CodeModel, GeneratedSymbol, ensure_unique_generated_symbols};

pub(super) fn validate_generated_symbols(
    _model: &CodeModel,
    names: &CppUnrealNames<'_>,
) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "root game data type",
            "game_data",
            names.struct_type_name("game_data")?,
        ),
        GeneratedSymbol::new(
            "lazy game data type",
            "lazy_game_data",
            names.lazy_game_data_type_name()?,
        ),
        GeneratedSymbol::new(
            "game data access interface",
            "config_forge_game_data_access",
            names.game_data_access_interface_name()?,
        ),
        GeneratedSymbol::new(
            "Blueprint facade type",
            "config_forge_blueprint_library",
            names.blueprint_library_type_name()?,
        ),
    ];
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
        if matches!(table.kind, crate::code_artifact::CodeTableKind::Collection) {
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
            if matches!(field.code_type, crate::code_artifact::CodeTypeRef::Text) {
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
            if matches!(field.code_type, crate::code_artifact::CodeTypeRef::Text) {
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
    ensure_unique_generated_symbols("cpp_unreal", "lazy game data members", lazy_symbols)?;

    let mut blueprint_symbols = Vec::new();
    for table in &_model.tables {
        if !matches!(table.kind, crate::code_artifact::CodeTableKind::Collection) {
            continue;
        }
        for index in &table.indexes {
            blueprint_symbols.push(GeneratedSymbol::new(
                "Blueprint index lookup function",
                format!("{}.{}", table.name, index.name),
                names.blueprint_index_function_name(table, index)?,
            ));
            if index.ordered {
                blueprint_symbols.push(GeneratedSymbol::new(
                    "Blueprint ordered range function",
                    format!("{}.{}", table.name, index.name),
                    names.blueprint_range_index_function_name(table, index)?,
                ));
            }
        }
    }
    ensure_unique_generated_symbols(
        "cpp_unreal",
        "Blueprint facade functions",
        blueprint_symbols,
    )?;
    Ok(())
}
