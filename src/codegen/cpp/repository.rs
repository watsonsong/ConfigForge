// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    codegen::{
        CodeIndex, CodeModel, CodeTable, CodeTableKind, CodeWriter,
        ir::{CodeForeignKey, CodeRelationCardinality},
    },
};

use super::types::CppNames;

pub(super) fn render_table_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let row_reader_function = names.reader_function_name(&row_type)?;

    content.raw_fmt(format_args!(
        "{} {}::Load(IRuntimeDataLoader& loader) {{\n",
        repository_type, repository_type
    ));
    content.raw("    auto root = loader.Open(DataFile);\n");
    if table.schema_revision.is_some() {
        content.raw(concat!(
            "    if (root->SchemaRevision() && *root->SchemaRevision() != ExpectedSchemaRevision) {\n",
            "        throw std::runtime_error(\"schema revision mismatch for runtime data\");\n",
            "    }\n",
        ));
    }
    content.raw("    auto rows_scope = EnterRequiredField(*root, \"rows\");\n");
    content.raw("    const std::size_t row_count = root->ReadRowCount();\n");
    content.raw_fmt(format_args!("    std::vector<{}> rows;\n", row_type));
    content.raw("    rows.reserve(row_count);\n");
    content.raw("    for (std::size_t row_index = 0; row_index < row_count; ++row_index) {\n");
    content.raw("        root->EnterIndex(row_index);\n");
    content.raw("        ReaderScope row_scope(*root);\n");
    content.raw_fmt(format_args!(
        "        rows.push_back({row_reader_function}(*root));\n"
    ));
    content.raw("    }\n");
    content.raw("    root->EnsureExhausted();\n");
    content.raw("    return FromRows(std::move(rows));\n");
    content.raw("}\n\n");
    render_from_rows_source(content, table, names, &repository_type, &row_type)
}

fn render_from_rows_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
    repository_type: &str,
    row_type: &str,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "{} {}::FromRows(std::vector<{}> rows) {{\n",
        repository_type, repository_type, row_type
    ));
    content.raw_fmt(format_args!("    {} table;\n", repository_type));
    content.raw("    table.rows_ = std::move(rows);\n");
    content.raw(
        "    for (std::size_t row_index = 0; row_index < table.rows_.size(); ++row_index) {\n",
    );
    content.raw("        const auto& row = table.rows_[row_index];\n");
    for index in &table.indexes {
        let storage_name = names.index_storage_name(index)?;
        let key = names.cpp_row_key_expr(table, index, "row")?;
        if index.unique {
            content.raw_fmt(format_args!(
                "        auto inserted_{storage_name} = table.{storage_name}.emplace({key}, row_index);\n"
            ));
            content.raw_fmt(format_args!(
                "        if (!inserted_{storage_name}.second) {{ throw std::runtime_error(\"Duplicate unique index key for {}.{}\"); }}\n",
                repository_type, index.name
            ));
        } else {
            content.raw_fmt(format_args!(
                "        table.{storage_name}[{key}].push_back(row_index);\n"
            ));
        }
    }
    content.raw("    }\n");
    content.raw("    return table;\n");
    content.raw("}\n\n");

    for index in &table.indexes {
        render_index_function_source(content, table, index, names)?;
        if index.unique {
            render_index_of_function_source(content, table, index, names)?;
        }
    }
    render_table_link_source(content, table, names)?;
    Ok(())
}

fn render_index_function_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let function_name = names.index_function_name(index)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;

    if index.unique {
        content.raw_fmt(format_args!(
            "const {}* {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.raw_fmt(format_args!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.raw_fmt(format_args!(
            "    if (iter == {}.end()) {{ return nullptr; }}\n",
            storage_name
        ));
        content.raw("    return &rows_[iter->second];\n");
        content.raw("}\n\n");
    } else {
        content.raw_fmt(format_args!(
            "std::vector<const {}*> {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.raw_fmt(format_args!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.raw_fmt(format_args!(
            "    if (iter == {}.end()) {{ return {{}}; }}\n",
            storage_name
        ));
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> result;\n",
            row_type
        ));
        content.raw("    result.reserve(iter->second.size());\n");
        content.raw("    for (std::size_t row_index : iter->second) {\n");
        content.raw("        result.push_back(&rows_[row_index]);\n");
        content.raw("    }\n");
        content.raw("    return result;\n");
        content.raw("}\n\n");
    }
    Ok(())
}

fn render_index_of_function_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    let repository_type = names.repository_type_name(&table.name)?;
    let function_name = names.index_of_function_name(index)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;

    content.raw_fmt(format_args!(
        "std::optional<std::size_t> {}::{}({}) const {{\n",
        repository_type, function_name, params
    ));
    content.raw_fmt(format_args!(
        "    auto iter = {}.find({});\n",
        storage_name, key
    ));
    content.raw_fmt(format_args!(
        "    if (iter == {}.end()) {{ return std::nullopt; }}\n",
        storage_name
    ));
    content.raw("    return iter->second;\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_table_link_source(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    if !table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        return Ok(());
    }
    let repository_type = names.repository_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "void {}::Link(const GameData& data) const {{\n",
        repository_type
    ));
    content.raw("    for (const auto& row : rows_) {\n");
    content.raw("        row.Link(data);\n");
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_game_data_load_source(
    content: &mut CodeWriter,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw("GameData GameData::Load(IRuntimeDataLoader& loader) {\n");
    content.raw("    GameData data;\n");
    for table in &model.tables {
        let member_name = names.member_name(&table.name)?;
        match table.kind {
            CodeTableKind::Collection => content.raw_fmt(format_args!(
                "    data.{member_name} = {}::Load(loader);\n",
                names.repository_type_name(&table.name)?
            )),
            CodeTableKind::Object => {
                let row_type = names.row_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "    auto root_{member_name} = loader.Open({row_type}::DataFile);\n"
                ));
                if table.schema_revision.is_some() {
                    content.raw_fmt(format_args!(
                        concat!(
                            "    if (root_{member_name}->SchemaRevision() &&\n",
                            "        *root_{member_name}->SchemaRevision() != {row_type}::ExpectedSchemaRevision) {{\n",
                            "        throw std::runtime_error(\"schema revision mismatch for runtime data\");\n",
                            "    }}\n",
                        ),
                        member_name = member_name,
                        row_type = row_type,
                    ));
                }
                content.raw_fmt(format_args!(
                    "    data.{member_name} = {}(*root_{member_name});\n",
                    names.reader_function_name(&row_type)?
                ));
                content.raw_fmt(format_args!("    root_{member_name}->EnsureExhausted();\n"));
            }
        }
    }
    content.raw("    return data;\n");
    content.raw("}\n");
    render_game_data_link_all_source(content, model, names)?;
    Ok(())
}

fn render_game_data_link_all_source(
    content: &mut CodeWriter,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw("\nvoid GameData::LinkAll() const {\n");
    for table in &model.tables {
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "    {}_.Link(*this);\n",
                    names.field_name(&table.name)?
                )),
                CodeTableKind::Object => content.raw_fmt(format_args!(
                    "    {}_.Link(*this);\n",
                    names.field_name(&table.name)?
                )),
            }
        }
    }
    content.raw("}\n");
    Ok(())
}

pub(super) fn render_compound_index_key_structs(
    content: &mut CodeWriter,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.len() <= 1 {
                continue;
            }

            content.raw_fmt(format_args!(
                "struct {} {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            for field in &index.fields {
                content.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            content.newline();
            content.raw_fmt(format_args!(
                "    bool operator<(const {}& other) const {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            content.raw("        return std::tie(");
            content.raw(
                index
                    .fields
                    .iter()
                    .map(|field| names.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.raw(") < std::tie(");
            content.raw(
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("other.{}", names.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.raw(");\n");
            content.raw("    }\n");
            content.raw("};\n\n");
        }
    }
    Ok(())
}

pub(super) fn render_table(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "class {} {{\npublic:\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    static constexpr const char* DataFile = {:?};\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "    static constexpr std::uint64_t ExpectedSchemaRevision = {revision}ULL;\n"
        ));
    }
    content.raw_fmt(format_args!(
        "    static {} Load(IRuntimeDataLoader& loader);\n",
        names.repository_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    static {} FromRows(std::vector<{}> rows);\n",
        names.repository_type_name(&table.name)?,
        names.row_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    const std::vector<{}>& Rows() const {{ return rows_; }}\n",
        names.row_type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "    const {}* RowAt(std::size_t row_index) const {{ return row_index < rows_.size() ? &rows_[row_index] : nullptr; }}\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
        if index.unique {
            render_index_of_function(content, index, names)?;
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("    void Link(const GameData& data) const;\n");
    }
    content.raw("private:\n");
    content.raw_fmt(format_args!(
        "    std::vector<{}> rows_;\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.raw_fmt(format_args!(
            "    {} {};\n",
            names.cpp_index_storage(table, index)?,
            names.index_storage_name(index)?
        ));
    }
    content.raw("};\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut CodeWriter,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    const {}* {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    } else {
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    }
    Ok(())
}

fn render_index_of_function(
    content: &mut CodeWriter,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    std::optional<std::size_t> {}({}) const;\n",
        names.index_of_function_name(index)?,
        names.cpp_index_params(index)?
    ));
    Ok(())
}

pub(super) fn render_row_relation_declarations(
    content: &mut CodeWriter,
    model: &CodeModel,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_table = table_by_name(model, &foreign_key.target_table)?;
        let target_row_type = names.row_type_name(&target_table.name)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One => content.raw_fmt(format_args!(
                "    const {}& {}(const GameData& data) const;\n",
                target_row_type, function_name
            )),
            CodeRelationCardinality::OptionalOne => content.raw_fmt(format_args!(
                "    const {}* {}(const GameData& data) const;\n",
                target_row_type, function_name
            )),
            CodeRelationCardinality::Many => content.raw_fmt(format_args!(
                "    std::vector<const {}*> {}(const GameData& data) const;\n",
                target_row_type, function_name
            )),
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("    void Link(const GameData& data) const;\n");
    }
    Ok(())
}

pub(super) fn render_row_relation_cache_members(
    content: &mut CodeWriter,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let cache_type = match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::Many => "LinkListCache",
            _ => "LinkCache",
        };
        content.raw_fmt(format_args!(
            "    mutable {} {};\n",
            cache_type,
            names.relation_cache_member_name(foreign_key)?
        ));
    }
    Ok(())
}

pub(super) fn render_row_relation_methods(
    content: &mut CodeWriter,
    model: &CodeModel,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    for foreign_key in table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
    {
        let target_table = table_by_name(model, &foreign_key.target_table)?;
        let target_index = index_by_name(target_table, &foreign_key.target_index)?;
        let target_row_type = names.row_type_name(&target_table.name)?;
        let function_name = names.relation_function_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .expect("linked field")
            .cardinality
        {
            CodeRelationCardinality::One => render_single_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
                false,
            )?,
            CodeRelationCardinality::OptionalOne => render_single_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
                true,
            )?,
            CodeRelationCardinality::Many => render_many_relation_method(
                content,
                table,
                foreign_key,
                target_table,
                target_index,
                names,
                &target_row_type,
                &function_name,
            )?,
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        let row_type = names.row_type_name(&table.name)?;
        content.raw_fmt(format_args!(
            "void {}::Link(const GameData& data) const {{\n",
            row_type
        ));
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "    static_cast<void>({}(data));\n",
                names.relation_function_name(foreign_key)?
            ));
        }
        content.raw("}\n\n");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_single_relation_method(
    content: &mut CodeWriter,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &CppNames<'_>,
    target_row_type: &str,
    function_name: &str,
    optional: bool,
) -> Result<()> {
    let source_row_type = names.row_type_name(&source_table.name)?;
    let cache_name = names.relation_cache_member_name(foreign_key)?;
    let target_accessor = names.type_name(&target_table.name)?;
    let index_of_name = names.index_of_function_name(target_index)?;
    let return_type = if optional {
        format!("const {target_row_type}*")
    } else {
        format!("const {target_row_type}&")
    };
    content.raw_fmt(format_args!(
        "{} {}::{}(const GameData& data) const {{\n",
        return_type, source_row_type, function_name
    ));
    content.raw_fmt(format_args!(
        "    if ({}.state == LinkCache::State::Empty) {{\n",
        cache_name
    ));
    if optional {
        content.raw("        return nullptr;\n");
    } else {
        content.raw_fmt(format_args!(
            "        throw std::runtime_error({:?});\n",
            format!(
                "required relation `{}` unexpectedly cached empty",
                foreign_key.name
            )
        ));
    }
    content.raw("    }\n");
    content.raw_fmt(format_args!(
        "    if ({}.state == LinkCache::State::RowIndex) {{\n",
        cache_name
    ));
    content.raw_fmt(format_args!(
        "        const auto* row = data.{}().RowAt({}.row_index);\n",
        target_accessor, cache_name
    ));
    content.raw_fmt(format_args!(
        "        if (row == nullptr) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "cached relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    if optional {
        content.raw("        return row;\n");
    } else {
        content.raw("        return *row;\n");
    }
    content.raw("    }\n");

    if optional {
        render_optional_empty_check(content, foreign_key, names, &cache_name)?;
    }
    let value_exprs = relation_value_exprs(foreign_key, names)?;
    let lookup_key = names.cpp_relation_lookup_key(target_table, target_index, &value_exprs)?;
    content.raw_fmt(format_args!(
        "    auto row_index = data.{}().{}({});\n",
        target_accessor, index_of_name, lookup_key
    ));
    content.raw_fmt(format_args!(
        "    if (!row_index) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw_fmt(format_args!(
        "    {}.state = LinkCache::State::RowIndex;\n",
        cache_name
    ));
    content.raw_fmt(format_args!("    {}.row_index = *row_index;\n", cache_name));
    content.raw_fmt(format_args!(
        "    const auto* row = data.{}().RowAt(*row_index);\n",
        target_accessor
    ));
    content.raw_fmt(format_args!(
        "    if (row == nullptr) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "resolved relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    if optional {
        content.raw("    return row;\n");
    } else {
        content.raw("    return *row;\n");
    }
    content.raw("}\n\n");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_many_relation_method(
    content: &mut CodeWriter,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &CppNames<'_>,
    target_row_type: &str,
    function_name: &str,
) -> Result<()> {
    let source_row_type = names.row_type_name(&source_table.name)?;
    let cache_name = names.relation_cache_member_name(foreign_key)?;
    let target_accessor = names.type_name(&target_table.name)?;
    let index_of_name = names.index_of_function_name(target_index)?;
    let source_field = foreign_key.fields.first().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "foreign key `{}` has no source fields",
            foreign_key.name
        ))
    })?;
    let source_field_name = names.field_name(&source_field.name)?;
    content.raw_fmt(format_args!(
        "std::vector<const {}*> {}::{}(const GameData& data) const {{\n",
        target_row_type, source_row_type, function_name
    ));
    content.raw_fmt(format_args!("    if (!{}.resolved) {{\n", cache_name));
    if source_field.required {
        content.raw_fmt(format_args!(
            "        for (const auto& key : {}) {{\n",
            source_field_name
        ));
    } else {
        content.raw_fmt(format_args!(
            "        if (!{}) {{ {}.resolved = true; }} else {{\n",
            source_field_name, cache_name
        ));
        content.raw_fmt(format_args!(
            "        for (const auto& key : *{}) {{\n",
            source_field_name
        ));
    }
    content.raw_fmt(format_args!(
        "            auto row_index = data.{}().{}(key);\n",
        target_accessor, index_of_name
    ));
    content.raw_fmt(format_args!(
        "            if (!row_index) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve list target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw_fmt(format_args!(
        "            {}.row_indexes.push_back(*row_index);\n",
        cache_name
    ));
    content.raw("        }\n");
    if !source_field.required {
        content.raw("        }\n");
    }
    content.raw_fmt(format_args!("        {}.resolved = true;\n", cache_name));
    content.raw("    }\n");
    content.raw_fmt(format_args!(
        "    std::vector<const {}*> rows;\n",
        target_row_type
    ));
    content.raw_fmt(format_args!(
        "    rows.reserve({}.row_indexes.size());\n",
        cache_name
    ));
    content.raw_fmt(format_args!(
        "    for (std::size_t row_index : {}.row_indexes) {{\n",
        cache_name
    ));
    content.raw_fmt(format_args!(
        "        const auto* row = data.{}().RowAt(row_index);\n",
        target_accessor
    ));
    content.raw_fmt(format_args!(
        "        if (row == nullptr) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "resolved list relation `{}` row index out of bounds",
            foreign_key.name
        )
    ));
    content.raw("        rows.push_back(row);\n");
    content.raw("    }\n");
    content.raw("    return rows;\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_optional_empty_check(
    content: &mut CodeWriter,
    foreign_key: &CodeForeignKey,
    names: &CppNames<'_>,
    cache_name: &str,
) -> Result<()> {
    let optional_fields = foreign_key
        .fields
        .iter()
        .filter(|field| !field.required)
        .collect::<Vec<_>>();
    if optional_fields.is_empty() {
        return Ok(());
    }
    let empty_conditions = optional_fields
        .iter()
        .map(|field| Ok(format!("!{}", names.field_name(&field.name)?)))
        .collect::<Result<Vec<_>>>()?;
    if optional_fields.len() == foreign_key.fields.len() {
        content.raw_fmt(format_args!(
            "    if ({}) {{ {}.state = LinkCache::State::Empty; return nullptr; }}\n",
            empty_conditions.join(" && "),
            cache_name
        ));
        if foreign_key.fields.len() > 1 {
            content.raw_fmt(format_args!(
                "    if ({}) {{ throw std::runtime_error({:?}); }}\n",
                empty_conditions.join(" || "),
                format!(
                    "foreign key relation `{}` has partial empty source key",
                    foreign_key.name
                )
            ));
        }
    } else {
        content.raw_fmt(format_args!(
            "    if ({}) {{ throw std::runtime_error({:?}); }}\n",
            empty_conditions.join(" || "),
            format!(
                "foreign key relation `{}` has partial empty source key",
                foreign_key.name
            )
        ));
    }
    Ok(())
}

fn relation_value_exprs(foreign_key: &CodeForeignKey, names: &CppNames<'_>) -> Result<Vec<String>> {
    foreign_key
        .fields
        .iter()
        .map(|field| {
            let field_name = names.field_name(&field.name)?;
            if field.required {
                Ok(field_name)
            } else {
                Ok(format!("*{field_name}"))
            }
        })
        .collect()
}

fn table_by_name<'a>(model: &'a CodeModel, name: &str) -> Result<&'a CodeTable> {
    model
        .tables
        .iter()
        .find(|table| table.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "cpp codegen foreign key references unknown target table `{name}`"
            ))
        })
}

fn index_by_name<'a>(table: &'a CodeTable, name: &str) -> Result<&'a CodeIndex> {
    table
        .indexes
        .iter()
        .find(|index| index.name == name)
        .ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "cpp codegen foreign key references unknown target index `{}.{}`",
                table.name, name
            ))
        })
}
