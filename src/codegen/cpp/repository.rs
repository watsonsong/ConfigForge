// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    codegen::{CodeIndex, CodeModel, CodeTable, CodeTableKind, CodeWriter},
    exporter::BINARY_FORMAT,
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
    if table.data_format == BINARY_FORMAT {
        content.raw(concat!(
            "    if (!root->SchemaRevision() || *root->SchemaRevision() != ExpectedSchemaRevision) {\n",
            "        throw std::runtime_error(\"schema revision mismatch for binary runtime data\");\n",
            "    }\n",
        ));
        content.raw("    const std::size_t row_count = root->ReadRowCount();\n");
        content.raw_fmt(format_args!("    std::vector<{}> rows;\n", row_type));
        content.raw("    for (std::size_t row_index = 0; row_index < row_count; ++row_index) {\n");
        content.raw_fmt(format_args!(
            "        rows.push_back({row_reader_function}(*root));\n"
        ));
        content.raw("    }\n");
        content.raw("    root->EnsureExhausted();\n");
        content.raw("    return FromRows(std::move(rows));\n");
        content.raw("}\n\n");
    } else {
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
        content.raw("    return FromRows(std::move(rows));\n");
        content.raw("}\n\n");
    }
    if table.data_format == BINARY_FORMAT {
        // named path has already been emitted in the `else` branch.
    } else {
        return render_from_rows_source(content, table, names, &repository_type, &row_type);
    }
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
    }
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
                if table.data_format == BINARY_FORMAT {
                    let row_type = names.row_type_name(&table.name)?;
                    content.raw_fmt(format_args!(
                        "    auto root_{member_name} = loader.Open({row_type}::DataFile);\n"
                    ));
                    content.raw_fmt(format_args!(
                        concat!(
                            "    if (!root_{member_name}->SchemaRevision() ||\n",
                            "        *root_{member_name}->SchemaRevision() != {row_type}::ExpectedSchemaRevision) {{\n",
                            "        throw std::runtime_error(\"schema revision mismatch for binary runtime data\");\n",
                            "    }}\n",
                        ),
                        member_name = member_name,
                        row_type = row_type,
                    ));
                    content.raw_fmt(format_args!(
                        "    data.{member_name} = {}(*root_{member_name});\n",
                        names.reader_function_name(&row_type)?
                    ));
                    content.raw_fmt(format_args!("    root_{member_name}->EnsureExhausted();\n"));
                } else {
                    content.raw_fmt(format_args!(
                        "    data.{member_name} = {}(*loader.Open({}::DataFile));\n",
                        names.reader_function_name(&names.row_type_name(&table.name)?)?,
                        names.row_type_name(&table.name)?
                    ));
                }
            }
        }
    }
    content.raw("    return data;\n");
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
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
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
