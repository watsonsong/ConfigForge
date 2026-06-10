// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    ConfigForgeError, Result,
    code_artifact::{
        CodeIndex, CodeModel, CodeTable, CodeTableKind, CodeWriterLike,
        ir::{CodeForeignKey, CodeRelationCardinality, relation_key_plan},
    },
    diagnostic::code::CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
};

use super::types::CppNames;

pub(super) fn render_table_source(
    content: &mut impl CodeWriterLike,
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
    content: &mut impl CodeWriterLike,
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
    if !table.indexes.is_empty() {
        content.raw(
            "    for (std::size_t row_index = 0; row_index < table.rows_.size(); ++row_index) {\n",
        );
        content.raw("        const auto& row = table.rows_[row_index];\n");
        for index in &table.indexes {
            let storage_name = names.index_storage_name(index)?;
            let key = names.cpp_row_key_expr(table, index, "row")?;
            if index.ordered {
                content.raw_fmt(format_args!(
                    "        table.{storage_name}.push_back(std::make_pair({key}, row_index));\n"
                ));
            } else if index.unique {
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
        for index in table.indexes.iter().filter(|index| index.ordered) {
            let storage_name = names.index_storage_name(index)?;
            content.raw_fmt(format_args!(
                "    std::stable_sort(table.{storage_name}.begin(), table.{storage_name}.end(), [](const auto& left, const auto& right) {{ return left.first < right.first; }});\n"
            ));
            if index.unique {
                content.raw_fmt(format_args!(
                    "    for (std::size_t index = 1; index < table.{storage_name}.size(); ++index) {{\n"
                ));
                content.raw_fmt(format_args!(
                    "        if (table.{storage_name}[index - 1].first == table.{storage_name}[index].first) {{ throw std::runtime_error(\"Duplicate unique index key for {}.{}\"); }}\n",
                    repository_type, index.name
                ));
                content.raw("    }\n");
            }
        }
    }
    content.raw("    return table;\n");
    content.raw("}\n\n");

    for index in &table.indexes {
        render_index_function_source(content, table, index, names)?;
    }
    render_table_link_source(content, table, names)?;
    Ok(())
}

fn render_index_function_source(
    content: &mut impl CodeWriterLike,
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

    if index.ordered {
        if index.unique {
            content.raw_fmt(format_args!(
                "const {}* {}::{}({}) const {{\n",
                row_type, repository_type, function_name, params
            ));
            content.raw_fmt(format_args!(
                "    auto iter = std::lower_bound({storage_name}.begin(), {storage_name}.end(), {key}, [](const auto& entry, const auto& value) {{ return entry.first < value; }});\n"
            ));
            content.raw_fmt(format_args!(
                "    if (iter == {storage_name}.end() || !(iter->first == {key})) {{ return nullptr; }}\n"
            ));
            content.raw("    return &rows_[iter->second];\n");
            content.raw("}\n\n");
        } else {
            content.raw_fmt(format_args!(
                "std::vector<const {}*> {}::{}({}) const {{\n",
                row_type, repository_type, function_name, params
            ));
            content.raw_fmt(format_args!(
                "    auto iter = std::lower_bound({storage_name}.begin(), {storage_name}.end(), {key}, [](const auto& entry, const auto& value) {{ return entry.first < value; }});\n"
            ));
            content.raw_fmt(format_args!(
                "    std::vector<const {}*> result;\n",
                row_type
            ));
            content.raw_fmt(format_args!(
                "    while (iter != {storage_name}.end() && iter->first == {key}) {{\n"
            ));
            content.raw("        result.push_back(&rows_[iter->second]);\n");
            content.raw("        ++iter;\n");
            content.raw("    }\n");
            content.raw("    return result;\n");
            content.raw("}\n\n");
        }
        render_ordered_index_function_sources(content, table, index, names)?;
    } else if index.unique {
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

fn render_ordered_index_function_sources(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let storage_name = names.index_storage_name(index)?;
    let key = names.cpp_lookup_key_expr(table, index)?;
    let params = names.cpp_index_params(index)?;
    let lower_name = names.lower_bound_index_function_name(index)?;
    let upper_name = names.upper_bound_index_function_name(index)?;
    let range_name = names.range_index_function_name(index)?;
    let range_params = names.cpp_range_index_params(table, index)?;

    content.raw_fmt(format_args!(
        "const {}* {}::{}({}) const {{\n",
        row_type, repository_type, lower_name, params
    ));
    content.raw_fmt(format_args!(
        "    auto iter = std::lower_bound({storage_name}.begin(), {storage_name}.end(), {key}, [](const auto& entry, const auto& value) {{ return entry.first < value; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    if (iter == {storage_name}.end()) {{ return nullptr; }}\n"
    ));
    content.raw("    return &rows_[iter->second];\n");
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "const {}* {}::{}({}) const {{\n",
        row_type, repository_type, upper_name, params
    ));
    content.raw_fmt(format_args!(
        "    auto iter = std::upper_bound({storage_name}.begin(), {storage_name}.end(), {key}, [](const auto& value, const auto& entry) {{ return value < entry.first; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    if (iter == {storage_name}.end()) {{ return nullptr; }}\n"
    ));
    content.raw("    return &rows_[iter->second];\n");
    content.raw("}\n\n");

    content.raw_fmt(format_args!(
        "std::vector<const {}*> {}::{}({}) const {{\n",
        row_type, repository_type, range_name, range_params
    ));
    content.raw_fmt(format_args!(
        "    auto begin = std::lower_bound({storage_name}.begin(), {storage_name}.end(), lower, [](const auto& entry, const auto& value) {{ return entry.first < value; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    auto end = std::lower_bound({storage_name}.begin(), {storage_name}.end(), upper, [](const auto& entry, const auto& value) {{ return entry.first < value; }});\n"
    ));
    content.raw_fmt(format_args!(
        "    std::vector<const {}*> result;\n",
        row_type
    ));
    content.raw("    result.reserve(static_cast<std::size_t>(std::distance(begin, end)));\n");
    content.raw("    for (auto iter = begin; iter != end; ++iter) {\n");
    content.raw("        result.push_back(&rows_[iter->second]);\n");
    content.raw("    }\n");
    content.raw("    return result;\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_table_link_source(
    content: &mut impl CodeWriterLike,
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
        "void {}::Link(const GameDataAccess& data) const {{\n",
        repository_type
    ));
    content.raw("    for (const auto& row : rows_) {\n");
    content.raw("        row.Link(data);\n");
    content.raw("    }\n");
    content.raw("}\n\n");
    Ok(())
}

pub(super) fn render_game_data_load_source(
    content: &mut impl CodeWriterLike,
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

pub(super) fn render_lazy_game_data_source(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    content.raw("\nLazyGameData::LazyGameData(std::shared_ptr<IRuntimeDataLoader> loader) : loader_(std::move(loader)) {\n");
    content.raw("    if (!loader_) { throw std::runtime_error(\"LazyGameData requires a runtime data loader\"); }\n");
    content.raw("}\n\n");
    for table in &model.tables {
        let table_type = names.table_data_type_name(table)?;
        let accessor_name = names.type_name(&table.name)?;
        let member_name = names.member_name(&table.name)?;
        content.raw_fmt(format_args!(
            "const {table_type}& LazyGameData::{accessor_name}() const {{\n"
        ));
        content.raw_fmt(format_args!("    if (!{member_name}) {{\n"));
        match table.kind {
            CodeTableKind::Collection => content.raw_fmt(format_args!(
                "        {member_name}.emplace({}::Load(*loader_));\n",
                names.repository_type_name(&table.name)?
            )),
            CodeTableKind::Object => {
                let row_type = names.row_type_name(&table.name)?;
                content.raw_fmt(format_args!(
                    "        auto root = loader_->Open({row_type}::DataFile);\n"
                ));
                if table.schema_revision.is_some() {
                    content.raw_fmt(format_args!(
                        concat!(
                            "        if (root->SchemaRevision() &&\n",
                            "            *root->SchemaRevision() != {row_type}::ExpectedSchemaRevision) {{\n",
                            "            throw std::runtime_error(\"schema revision mismatch for runtime data\");\n",
                            "        }}\n",
                        ),
                        row_type = row_type,
                    ));
                }
                content.raw_fmt(format_args!(
                    "        {member_name}.emplace({}(*root));\n",
                    names.reader_function_name(&row_type)?
                ));
                content.raw("        root->EnsureExhausted();\n");
            }
        }
        content.raw("    }\n");
        content.raw_fmt(format_args!("    return *{member_name};\n"));
        content.raw("}\n\n");
    }
    content.raw("void LazyGameData::PreloadAll() const {\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "    (void){}();\n",
            names.type_name(&table.name)?
        ));
    }
    content.raw("}\n");
    content.raw("\nvoid LazyGameData::LinkAll() const {\n");
    content.raw("    PreloadAll();\n");
    for table in &model.tables {
        if table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
        {
            content.raw_fmt(format_args!(
                "    {}().Link(*this);\n",
                names.type_name(&table.name)?
            ));
        }
    }
    content.raw("}\n");
    Ok(())
}

fn render_game_data_link_all_source(
    content: &mut impl CodeWriterLike,
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
    content: &mut impl CodeWriterLike,
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
                "    bool operator==(const {}& other) const {{\n",
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
            content.raw(") == std::tie(");
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

            content.raw_fmt(format_args!(
                "struct {} {{\n",
                names.compound_index_key_hash_type_name(table, index)?
            ));
            content.raw_fmt(format_args!(
                "    std::size_t operator()(const {}& value) const {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            content.raw("        std::size_t seed = 0;\n");
            for field in &index.fields {
                let field_type = names.cpp_field_type(field)?;
                let field_name = names.field_name(&field.name)?;
                content.raw_fmt(format_args!(
                    "        seed ^= std::hash<{field_type}>{{}}(value.{field_name}) + 0x9e3779b97f4a7c15ULL + (seed << 6) + (seed >> 2);\n"
                ));
            }
            content.raw("        return seed;\n");
            content.raw("    }\n");
            content.raw("};\n\n");
        }
    }
    Ok(())
}

pub(super) fn render_table(
    content: &mut impl CodeWriterLike,
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
    if table.preserve_source_order {
        content.raw("    std::size_t Size() const { return rows_.size(); }\n");
        content.raw_fmt(format_args!(
            "    const std::vector<{}>& Rows() const {{ return rows_; }}\n",
            names.row_type_name(&table.name)?
        ));
        content.raw_fmt(format_args!(
            "    const {}* RowAt(std::size_t row_index) const {{ return RowByHandle(row_index); }}\n",
            names.row_type_name(&table.name)?
        ));
    }
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("    void Link(const GameDataAccess& data) const;\n");
    }
    content.raw("private:\n");
    content.raw_fmt(format_args!(
        "    const {}* RowByHandle(std::size_t row_handle) const {{ return row_handle < rows_.size() ? &rows_[row_handle] : nullptr; }}\n",
        names.row_type_name(&table.name)?
    ));
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
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    let row_type = names.row_type_name(&table.name)?;
    if index.unique {
        content.raw_fmt(format_args!(
            "    const {}* {}({}) const;\n",
            row_type,
            names.index_function_name(index)?,
            params
        ));
    } else {
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> {}({}) const;\n",
            row_type,
            names.index_function_name(index)?,
            params
        ));
    }
    if index.ordered {
        content.raw_fmt(format_args!(
            "    const {}* {}({}) const;\n",
            row_type,
            names.lower_bound_index_function_name(index)?,
            params
        ));
        content.raw_fmt(format_args!(
            "    const {}* {}({}) const;\n",
            row_type,
            names.upper_bound_index_function_name(index)?,
            params
        ));
        content.raw_fmt(format_args!(
            "    std::vector<const {}*> {}({}) const;\n",
            row_type,
            names.range_index_function_name(index)?,
            names.cpp_range_index_params(table, index)?
        ));
    }
    Ok(())
}

pub(super) fn render_row_relation_declarations(
    content: &mut impl CodeWriterLike,
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
                "    const {}& {}(const GameDataAccess& data) const;\n",
                target_row_type, function_name
            )),
            CodeRelationCardinality::OptionalOne => content.raw_fmt(format_args!(
                "    const {}* {}(const GameDataAccess& data) const;\n",
                target_row_type, function_name
            )),
            CodeRelationCardinality::Many => content.raw_fmt(format_args!(
                "    std::vector<const {}*> {}(const GameDataAccess& data) const;\n",
                target_row_type, function_name
            )),
        }
    }
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("    void Link(const GameDataAccess& data) const;\n");
    }
    Ok(())
}

pub(super) fn render_row_relation_cache_members(
    content: &mut impl CodeWriterLike,
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
    content: &mut impl CodeWriterLike,
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
            "void {}::Link(const GameDataAccess& data) const {{\n",
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
    content: &mut impl CodeWriterLike,
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
    let index_function_name = names.index_function_name(target_index)?;
    let return_type = if optional {
        format!("const {target_row_type}*")
    } else {
        format!("const {target_row_type}&")
    };
    content.raw_fmt(format_args!(
        "{} {}::{}(const GameDataAccess& data) const {{\n",
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
    if optional {
        render_optional_empty_check(content, foreign_key, names, &cache_name)?;
    }
    let value_exprs = relation_value_exprs(foreign_key, names)?;
    content.raw_fmt(format_args!(
        "    const auto* row = data.{}().{}({});\n",
        target_accessor,
        index_function_name,
        value_exprs.join(", ")
    ));
    content.raw_fmt(format_args!(
        "    if (row == nullptr) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
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
    content: &mut impl CodeWriterLike,
    source_table: &CodeTable,
    foreign_key: &CodeForeignKey,
    target_table: &CodeTable,
    target_index: &CodeIndex,
    names: &CppNames<'_>,
    target_row_type: &str,
    function_name: &str,
) -> Result<()> {
    let source_row_type = names.row_type_name(&source_table.name)?;
    let target_accessor = names.type_name(&target_table.name)?;
    let index_function_name = names.index_function_name(target_index)?;
    let source_field = foreign_key.fields.first().ok_or_else(|| {
        codegen_relation_error(
            "generator.cpp.relations",
            format!("foreign key `{}` has no source fields", foreign_key.name),
            foreign_key.name.clone(),
            "foreign key with at least one source field",
            "Check linked foreign key source field declarations before C++ codegen.",
        )
    })?;
    let source_field_name = names.field_name(&source_field.name)?;
    content.raw_fmt(format_args!(
        "std::vector<const {}*> {}::{}(const GameDataAccess& data) const {{\n",
        target_row_type, source_row_type, function_name
    ));
    content.raw_fmt(format_args!(
        "    std::vector<const {}*> rows;\n",
        target_row_type
    ));
    if source_field.required {
        content.raw_fmt(format_args!(
            "    rows.reserve({}.size());\n",
            source_field_name
        ));
        content.raw_fmt(format_args!(
            "    for (const auto& key : {}) {{\n",
            source_field_name
        ));
    } else {
        content.raw_fmt(format_args!(
            "    if (!{}) {{ return rows; }}\n",
            source_field_name
        ));
        content.raw_fmt(format_args!(
            "    rows.reserve({}->size());\n",
            source_field_name
        ));
        content.raw_fmt(format_args!(
            "    for (const auto& key : *{}) {{\n",
            source_field_name
        ));
    }
    content.raw_fmt(format_args!(
        "        const auto* row = data.{}().{}(key);\n",
        target_accessor, index_function_name
    ));
    content.raw_fmt(format_args!(
        "        if (row == nullptr) {{ throw std::runtime_error({:?}); }}\n",
        format!(
            "foreign key relation `{}.{}` could not resolve list target `{}.{}`",
            source_table.name, foreign_key.name, target_table.name, target_index.name
        )
    ));
    content.raw("        rows.push_back(row);\n");
    content.raw("    }\n");
    content.raw("    return rows;\n");
    content.raw("}\n\n");
    Ok(())
}

fn render_optional_empty_check(
    content: &mut impl CodeWriterLike,
    foreign_key: &CodeForeignKey,
    names: &CppNames<'_>,
    cache_name: &str,
) -> Result<()> {
    let key_plan = relation_key_plan(foreign_key);
    if key_plan.optional_key_fields.is_empty() {
        return Ok(());
    }
    let empty_conditions = key_plan
        .optional_key_fields
        .iter()
        .map(|field| Ok(format!("!{}", names.field_name(&field.name)?)))
        .collect::<Result<Vec<_>>>()?;
    if key_plan.all_key_fields_optional {
        content.raw_fmt(format_args!(
            "    if ({}) {{ {}.state = LinkCache::State::Empty; return nullptr; }}\n",
            empty_conditions.join(" && "),
            cache_name
        ));
        if key_plan.should_report_partial_empty {
            content.raw_fmt(format_args!(
                "    if ({}) {{ throw std::runtime_error({:?}); }}\n",
                empty_conditions.join(" || "),
                format!(
                    "foreign key relation `{}` has partial empty source key",
                    foreign_key.name
                )
            ));
        }
    } else if key_plan.should_report_partial_empty {
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
            codegen_relation_error(
                "generator.cpp.relations",
                format!("cpp codegen foreign key references unknown target table `{name}`"),
                name,
                "target table present in code model",
                "Check foreign key target table visibility before C++ codegen.",
            )
        })
}

fn index_by_name<'a>(table: &'a CodeTable, name: &str) -> Result<&'a CodeIndex> {
    table
        .indexes
        .iter()
        .find(|index| index.name == name)
        .ok_or_else(|| {
            codegen_relation_error(
                "generator.cpp.relations",
                format!(
                    "cpp codegen foreign key references unknown target index `{}.{}`",
                    table.name, name
                ),
                format!("{}.{}", table.name, name),
                "target index present in code model",
                "Check foreign key target index visibility before C++ codegen.",
            )
        })
}

fn codegen_relation_error(
    source: &'static str,
    message: String,
    current: impl Into<String>,
    expected: impl Into<String>,
    hint: impl Into<String>,
) -> ConfigForgeError {
    crate::code_artifact::generator_option_error(
        CF_GENERATOR_UNSUPPORTED_DATA_INPUT,
        source,
        message,
        current,
        expected,
        hint,
    )
}
