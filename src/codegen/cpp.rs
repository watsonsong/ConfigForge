use crate::{ConfigForgeError, Result};

use super::{
    ArtifactKind, CodeEnum, CodeExportInput, CodeExportLanguage, CodeFragmentPayload,
    CodeGenerator, CodeIndex, CodeModel, CodeStruct, CodeTable, CodeTableKind, CodeTypeRef,
    CodegenOptions, GeneratedFile, GeneratedSymbol, build_code_model,
    ensure_unique_generated_symbols, ir::require_non_empty_index_fields, naming,
    render_file_header,
};

pub struct CppCodeGenerator;

impl CodeGenerator for CppCodeGenerator {
    fn language(&self) -> CodeExportLanguage {
        CodeExportLanguage::Cpp
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let cpp_options = input.target.options.cpp()?;
        let names = CppNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut header = render_file_header(input);
        header.push_str("#pragma once\n\n");
        header.push_str("#include <cstdint>\n#include <cstddef>\n#include <map>\n#include <memory>\n#include <optional>\n#include <string>\n#include <tuple>\n#include <unordered_map>\n#include <utility>\n#include <vector>\n\n");
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        if let Some(namespace) = namespace {
            header.push_str(&format!("namespace {namespace} {{\n\n"));
        }
        header.push_str(
            "inline constexpr const char* CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\";\n\n",
        );
        header.push_str(
            "class IRuntimeDataReader {\npublic:\n    virtual ~IRuntimeDataReader() = default;\n    virtual std::unique_ptr<IRuntimeDataReader> Field(const std::string& name) const = 0;\n    virtual std::unique_ptr<IRuntimeDataReader> Index(std::size_t index) const = 0;\n    virtual std::size_t Size() const = 0;\n    virtual std::int64_t AsInt64() const = 0;\n    virtual double AsDouble() const = 0;\n    virtual bool AsBool() const = 0;\n    virtual std::string AsString() const = 0;\n    virtual bool IsNull() const = 0;\n};\n\nclass IRuntimeDataLoader {\npublic:\n    virtual ~IRuntimeDataLoader() = default;\n    virtual std::unique_ptr<IRuntimeDataReader> Open(const std::string& path) = 0;\n};\n\n",
        );

        for code_enum in &model.enums {
            push_doc(&mut header, code_enum.desc.as_deref());
            header.push_str(&format!(
                "enum class {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc_indented(&mut header, value.desc.as_deref(), "    ");
                header.push_str(&format!("    {},\n", names.enum_variant_name(&value.name)?));
            }
            header.push_str("};\n\n");
        }

        for code_struct in &model.structs {
            push_doc(&mut header, code_struct.desc.as_deref());
            header.push_str(&format!(
                "struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.push_str(&format!(
                    "    {} {};\n",
                    names.cpp_type(&field.code_type)?,
                    names.field_name(&field.name)?
                ));
            }
            header.push_str("};\n\n");
        }

        render_compound_index_key_structs(&mut header, &model, &names)?;

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut header, fragment.desc.as_deref());
                header.push_str(&format!(
                    "struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                    header.push_str(&format!(
                        "    {} {};\n",
                        names.cpp_type(&field.code_type)?,
                        names.field_name(&field.name)?
                    ));
                }
                header.push_str("};\n\n");
            }
        }

        for table in &model.tables {
            push_doc(&mut header, table.desc.as_deref());
            header.push_str(&format!(
                "struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            if table.kind == CodeTableKind::Object {
                header.push_str(&format!(
                    "    static constexpr const char* DataFile = {:?};\n",
                    table.data_file
                ));
            }
            for field in &table.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.push_str(&format!(
                    "    {} {};\n",
                    names.cpp_type(&field.code_type)?,
                    names.field_name(&field.name)?
                ));
            }
            for fragment in &table.fragments {
                header.push_str(&format!(
                    "    std::optional<{}> {};\n",
                    names.fragment_payload_type_name(table, fragment)?,
                    names.field_name(&fragment.name)?
                ));
            }
            header.push_str("};\n\n");
            if table.kind == CodeTableKind::Collection {
                render_table(&mut header, table, &names)?;
            }
        }

        header.push_str("class GameData {\npublic:\n");
        header.push_str("    static GameData Load(IRuntimeDataLoader& loader);\n");
        for table in &model.tables {
            header.push_str(&format!(
                "    const {}& {}() const {{ return {}; }}\n",
                names.table_data_type_name(table)?,
                names.type_name(&table.name)?,
                names.member_name(&table.name)?
            ));
        }
        header.push_str("private:\n");
        for table in &model.tables {
            header.push_str(&format!(
                "    {} {};\n",
                names.table_data_type_name(table)?,
                names.member_name(&table.name)?
            ));
        }
        header.push_str("};\n");
        if let Some(namespace) = namespace {
            header.push_str(&format!("\n}} // namespace {namespace}\n"));
        }

        let source = render_source_file(input, &model, namespace, &names)?;

        Ok(vec![
            GeneratedFile {
                path: input.target.out_dir.join("generated.hpp"),
                kind: ArtifactKind::GeneratedCode,
                content: header,
            },
            GeneratedFile {
                path: input.target.out_dir.join("generated.cpp"),
                kind: ArtifactKind::GeneratedCode,
                content: source,
            },
        ])
    }
}

fn validate_generated_symbols(model: &CodeModel, names: &CppNames<'_>) -> Result<()> {
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

fn render_source_file(
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
) -> Result<String> {
    let mut content = render_file_header(input);
    content.push_str("#include \"generated.hpp\"\n\n");
    content.push_str("#include <stdexcept>\n\n");
    if let Some(namespace) = namespace {
        content.push_str(&format!("namespace {namespace} {{\n\n"));
    }

    render_reader_helpers_source(&mut content);
    for code_enum in &model.enums {
        render_enum_reader_source(&mut content, code_enum, names)?;
    }
    for code_struct in &model.structs {
        render_struct_reader_source(&mut content, code_struct, names)?;
    }
    for table in &model.tables {
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                render_fragment_reader_source(&mut content, table, fragment, names)?;
            }
        }
    }
    for table in &model.tables {
        render_row_reader_source(&mut content, table, names)?;
    }
    content.push_str("} // namespace\n\n");

    for table in &model.tables {
        if table.kind == CodeTableKind::Collection {
            render_table_source(&mut content, table, names)?;
        }
    }
    render_game_data_load_source(&mut content, model, names)?;

    if let Some(namespace) = namespace {
        content.push_str(&format!("\n}} // namespace {namespace}\n"));
    }
    Ok(content)
}

fn render_reader_helpers_source(content: &mut String) {
    content.push_str(
        "namespace {\n\nstd::unique_ptr<IRuntimeDataReader> RequiredField(const IRuntimeDataReader& reader, const std::string& name) {\n    auto value = reader.Field(name);\n    if (!value || value->IsNull()) { throw std::runtime_error(\"missing required field `\" + name + \"`\"); }\n    return value;\n}\n\nstd::int64_t ReadRequiredInt64(const IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name)->AsInt64(); }\ndouble ReadRequiredDouble(const IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name)->AsDouble(); }\nbool ReadRequiredBool(const IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name)->AsBool(); }\nstd::string ReadRequiredString(const IRuntimeDataReader& reader, const std::string& name) { return RequiredField(reader, name)->AsString(); }\n\n",
    );
}

fn render_enum_reader_source(
    content: &mut String,
    code_enum: &CodeEnum,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_enum.name)?;
    let function_name = names.reader_function_name(&type_name)?;
    content.push_str(&format!(
        "{} {}(const IRuntimeDataReader& reader) {{\n",
        type_name, function_name
    ));
    content.push_str("    const std::string value = reader.AsString();\n");
    for value in &code_enum.values {
        content.push_str(&format!(
            "    if (value == {:?}) {{ return {}::{}; }}\n",
            value.name,
            type_name,
            names.enum_variant_name(&value.name)?
        ));
    }
    content.push_str(&format!(
        "    throw std::runtime_error(\"unknown {} value `\" + value + \"`\");\n",
        code_enum.name
    ));
    content.push_str("}\n\n");
    Ok(())
}

fn render_struct_reader_source(
    content: &mut String,
    code_struct: &CodeStruct,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_source(content, &type_name, &code_struct.fields, None, names)
}

fn render_fragment_reader_source(
    content: &mut String,
    table: &CodeTable,
    fragment: &super::CodeFragment,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_source(content, &type_name, &fragment.fields, None, names)
}

fn render_row_reader_source(
    content: &mut String,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    let type_name = names.row_type_name(&table.name)?;
    render_reader_function_source(content, &type_name, &table.fields, Some(table), names)
}

fn render_reader_function_source(
    content: &mut String,
    type_name: &str,
    fields: &[super::CodeField],
    table: Option<&CodeTable>,
    names: &CppNames<'_>,
) -> Result<()> {
    let function_name = names.reader_function_name(type_name)?;
    content.push_str(&format!(
        "{} {}(const IRuntimeDataReader& reader) {{\n",
        type_name, function_name
    ));
    content.push_str(&format!("    {} value{{}};\n", type_name));
    for field in fields {
        content.push_str(&format!(
            "    value.{} = {};\n",
            names.field_name(&field.name)?,
            names.cpp_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            let reader_function = names.reader_function_name(&payload_type)?;
            content.push_str(&format!(
                "    if (auto fragment = reader.Field({:?}); fragment && !fragment->IsNull()) {{ value.{field_name} = {reader_function}(*fragment); }}\n",
                fragment.name
            ));
        }
    }
    content.push_str("    return value;\n");
    content.push_str("}\n\n");
    Ok(())
}

fn render_table_source(
    content: &mut String,
    table: &CodeTable,
    names: &CppNames<'_>,
) -> Result<()> {
    let repository_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    let row_reader_function = names.reader_function_name(&row_type)?;

    content.push_str(&format!(
        "{} {}::Load(IRuntimeDataLoader& loader) {{\n",
        repository_type, repository_type
    ));
    content.push_str("    auto root = loader.Open(DataFile);\n");
    content.push_str("    auto rows_reader = RequiredField(*root, \"rows\");\n");
    content.push_str(&format!("    std::vector<{}> rows;\n", row_type));
    content.push_str("    rows.reserve(rows_reader->Size());\n");
    content.push_str(
        "    for (std::size_t row_index = 0; row_index < rows_reader->Size(); ++row_index) {\n",
    );
    content.push_str(&format!(
        "        rows.push_back({row_reader_function}(*rows_reader->Index(row_index)));\n"
    ));
    content.push_str("    }\n");
    content.push_str("    return FromRows(std::move(rows));\n");
    content.push_str("}\n\n");

    content.push_str(&format!(
        "{} {}::FromRows(std::vector<{}> rows) {{\n",
        repository_type, repository_type, row_type
    ));
    content.push_str(&format!("    {} table;\n", repository_type));
    content.push_str("    table.rows_ = std::move(rows);\n");
    content.push_str(
        "    for (std::size_t row_index = 0; row_index < table.rows_.size(); ++row_index) {\n",
    );
    content.push_str("        const auto& row = table.rows_[row_index];\n");
    for index in &table.indexes {
        let storage_name = names.index_storage_name(index)?;
        let key = names.cpp_row_key_expr(table, index, "row")?;
        if index.unique {
            content.push_str(&format!(
                "        auto inserted_{storage_name} = table.{storage_name}.emplace({key}, row_index);\n"
            ));
            content.push_str(&format!(
                "        if (!inserted_{storage_name}.second) {{ throw std::runtime_error(\"Duplicate unique index key for {}.{}\"); }}\n",
                repository_type, index.name
            ));
        } else {
            content.push_str(&format!(
                "        table.{storage_name}[{key}].push_back(row_index);\n"
            ));
        }
    }
    content.push_str("    }\n");
    content.push_str("    return table;\n");
    content.push_str("}\n\n");

    for index in &table.indexes {
        render_index_function_source(content, table, index, names)?;
    }
    Ok(())
}

fn render_index_function_source(
    content: &mut String,
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
        content.push_str(&format!(
            "const {}* {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.push_str(&format!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.push_str(&format!(
            "    if (iter == {}.end()) {{ return nullptr; }}\n",
            storage_name
        ));
        content.push_str("    return &rows_[iter->second];\n");
        content.push_str("}\n\n");
    } else {
        content.push_str(&format!(
            "std::vector<const {}*> {}::{}({}) const {{\n",
            row_type, repository_type, function_name, params
        ));
        content.push_str(&format!(
            "    auto iter = {}.find({});\n",
            storage_name, key
        ));
        content.push_str(&format!(
            "    if (iter == {}.end()) {{ return {{}}; }}\n",
            storage_name
        ));
        content.push_str(&format!("    std::vector<const {}*> result;\n", row_type));
        content.push_str("    result.reserve(iter->second.size());\n");
        content.push_str("    for (std::size_t row_index : iter->second) {\n");
        content.push_str("        result.push_back(&rows_[row_index]);\n");
        content.push_str("    }\n");
        content.push_str("    return result;\n");
        content.push_str("}\n\n");
    }
    Ok(())
}

fn render_game_data_load_source(
    content: &mut String,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    content.push_str("GameData GameData::Load(IRuntimeDataLoader& loader) {\n");
    content.push_str("    GameData data;\n");
    for table in &model.tables {
        let member_name = names.member_name(&table.name)?;
        match table.kind {
            CodeTableKind::Collection => content.push_str(&format!(
                "    data.{member_name} = {}::Load(loader);\n",
                names.repository_type_name(&table.name)?
            )),
            CodeTableKind::Object => content.push_str(&format!(
                "    data.{member_name} = {}(*loader.Open({}::DataFile));\n",
                names.reader_function_name(&names.row_type_name(&table.name)?)?,
                names.row_type_name(&table.name)?
            )),
        }
    }
    content.push_str("    return data;\n");
    content.push_str("}\n");
    Ok(())
}

fn render_compound_index_key_structs(
    content: &mut String,
    model: &CodeModel,
    names: &CppNames<'_>,
) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.len() <= 1 {
                continue;
            }

            content.push_str(&format!(
                "struct {} {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            for field in &index.fields {
                content.push_str(&format!(
                    "    {} {};\n",
                    names.cpp_type(&field.code_type)?,
                    names.field_name(&field.name)?
                ));
            }
            content.push('\n');
            content.push_str(&format!(
                "    bool operator<(const {}& other) const {{\n",
                names.compound_index_key_type_name(table, index)?
            ));
            content.push_str("        return std::tie(");
            content.push_str(
                &index
                    .fields
                    .iter()
                    .map(|field| names.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.push_str(") < std::tie(");
            content.push_str(
                &index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("other.{}", names.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", "),
            );
            content.push_str(");\n");
            content.push_str("    }\n");
            content.push_str("};\n\n");
        }
    }
    Ok(())
}

fn render_table(content: &mut String, table: &CodeTable, names: &CppNames<'_>) -> Result<()> {
    content.push_str(&format!(
        "class {} {{\npublic:\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "    static constexpr const char* DataFile = {:?};\n",
        table.data_file
    ));
    content.push_str(&format!(
        "    static {} Load(IRuntimeDataLoader& loader);\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "    static {} FromRows(std::vector<{}> rows);\n",
        names.repository_type_name(&table.name)?,
        names.row_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "    const std::vector<{}>& Rows() const {{ return rows_; }}\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    content.push_str("private:\n");
    content.push_str(&format!(
        "    std::vector<{}> rows_;\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.push_str(&format!(
            "    {} {};\n",
            names.cpp_index_storage(table, index)?,
            names.index_storage_name(index)?
        ));
    }
    content.push_str("};\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut String,
    table: &CodeTable,
    index: &CodeIndex,
    names: &CppNames<'_>,
) -> Result<()> {
    let params = names.cpp_index_params(index)?;
    if index.unique {
        content.push_str(&format!(
            "    const {}* {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    } else {
        content.push_str(&format!(
            "    std::vector<const {}*> {}({}) const;\n",
            names.row_type_name(&table.name)?,
            names.index_function_name(index)?,
            params
        ));
    }
    Ok(())
}

struct CppNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> CppNames<'a> {
    fn new(options: &'a CodegenOptions) -> Self {
        Self { options }
    }

    fn type_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.type_name)
    }

    fn row_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_row"))
    }

    fn repository_type_name(&self, table_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_table"))
    }

    fn table_data_type_name(&self, table: &CodeTable) -> Result<String> {
        match table.kind {
            CodeTableKind::Object => self.row_type_name(&table.name),
            CodeTableKind::Collection => self.repository_type_name(&table.name),
        }
    }

    fn fragment_type_name(&self, table_name: &str, fragment_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_{fragment_name}_fragment"))
    }

    fn fragment_payload_type_name(
        &self,
        table: &CodeTable,
        fragment: &super::CodeFragment,
    ) -> Result<String> {
        match &fragment.payload {
            CodeFragmentPayload::InlineFields => {
                self.fragment_type_name(&table.name, &fragment.name)
            }
            CodeFragmentPayload::Struct { name } => self.type_name(name),
        }
    }

    fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    fn member_name(&self, name: &str) -> Result<String> {
        Ok(format!("{}_", self.field_name(name)?))
    }

    fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        Ok(format!("{}_", self.field_name(&index.name)?))
    }

    fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    fn reader_function_name(&self, type_name: &str) -> Result<String> {
        self.identifier(&format!("read_{type_name}"), self.options.naming.function)
    }

    fn cpp_index_params(&self, index: &CodeIndex) -> Result<String> {
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "{} {}",
                    self.cpp_type(&field.code_type)?,
                    self.field_name(&field.name)?
                ))
            })
            .collect::<Result<Vec<_>>>()
            .map(|params| params.join(", "))
    }

    fn cpp_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "std::int64_t".to_owned(),
            CodeTypeRef::Float => "double".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                "std::string".to_owned()
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
        })
    }

    fn cpp_read_field_expr(&self, field: &super::CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("ReadRequiredInt64(reader, {source_name})"),
            CodeTypeRef::Float => format!("ReadRequiredDouble(reader, {source_name})"),
            CodeTypeRef::Bool => format!("ReadRequiredBool(reader, {source_name})"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("ReadRequiredString(reader, {source_name})")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => format!(
                "{}(*RequiredField(reader, {source_name}))",
                self.reader_function_name(&self.type_name(name)?)?
            ),
        })
    }

    fn cpp_index_storage(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        let key = self.cpp_key_type(table, index)?;
        let map_type = if index.fields.len() == 1 {
            "std::unordered_map"
        } else {
            "std::map"
        };
        if index.unique {
            Ok(format!("{map_type}<{key}, std::size_t>"))
        } else {
            Ok(format!("{map_type}<{key}, std::vector<std::size_t>>"))
        }
    }

    fn cpp_key_type(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.cpp_type(&index.fields[0].code_type)
        } else {
            self.compound_index_key_type_name(table, index)
        }
    }

    fn cpp_row_key_expr(
        &self,
        table: &CodeTable,
        index: &CodeIndex,
        row_expr: &str,
    ) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "{}.{}",
                row_expr,
                self.field_name(&index.fields[0].name)?
            ))
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("{}.{}", row_expr, self.field_name(&field.name)?))),
            )
        }
    }

    fn cpp_lookup_key_expr(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            self.cpp_compound_key_expr(
                table,
                index,
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name)),
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

    fn compound_index_key_type_name(&self, table: &CodeTable, index: &CodeIndex) -> Result<String> {
        self.type_name(&format!("{}_{}_key", table.name, index.name))
    }

    fn identifier(&self, value: &str, style: super::CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_cpp_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("cpp codegen: {message}")))
    }
}

fn push_doc(content: &mut String, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut String, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.push_str(indent);
            content.push_str("// ");
            content.push_str(line);
            content.push('\n');
        }
    }
}
