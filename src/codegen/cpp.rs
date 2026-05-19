use crate::{ConfigForgeError, Result};

use super::{
    build_code_model, ensure_unique_generated_symbols, ir::require_non_empty_index_fields, naming,
    render_file_header, ArtifactKind, CodeExportInput, CodeExportLanguage, CodeGenerator,
    CodeIndex, CodeModel, CodeTable, CodeTypeRef, CodegenOptions, GeneratedFile, GeneratedSymbol,
};

pub struct CppCodeGenerator;

impl CodeGenerator for CppCodeGenerator {
    fn language(&self) -> CodeExportLanguage {
        CodeExportLanguage::Cpp
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        reject_compound_indexes(&model)?;
        let cpp_options = input.target.options.cpp()?;
        let names = CppNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut content = render_file_header(input);
        content.push_str("#pragma once\n\n");
        content.push_str("#include <cstdint>\n#include <cstddef>\n#include <optional>\n#include <string>\n#include <unordered_map>\n#include <utility>\n#include <vector>\n\n");
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        if let Some(namespace) = namespace {
            content.push_str(&format!("namespace {namespace} {{\n\n"));
        }
        content.push_str("class IRuntimeDataLoader;\n\n");

        for code_enum in &model.enums {
            push_doc(&mut content, code_enum.desc.as_deref());
            content.push_str(&format!(
                "enum class {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc_indented(&mut content, value.desc.as_deref(), "    ");
                content.push_str(&format!("    {},\n", names.enum_variant_name(&value.name)?));
            }
            content.push_str("};\n\n");
        }

        for code_struct in &model.structs {
            push_doc(&mut content, code_struct.desc.as_deref());
            content.push_str(&format!(
                "struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "    ");
                content.push_str(&format!(
                    "    {} {};\n",
                    names.cpp_type(&field.code_type)?,
                    names.field_name(&field.name)?
                ));
            }
            content.push_str("};\n\n");
        }

        for table in &model.tables {
            for fragment in &table.fragments {
                push_doc(&mut content, fragment.desc.as_deref());
                content.push_str(&format!(
                    "struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut content, field.desc.as_deref(), "    ");
                    content.push_str(&format!(
                        "    {} {};\n",
                        names.cpp_type(&field.code_type)?,
                        names.field_name(&field.name)?
                    ));
                }
                content.push_str("};\n\n");
            }
        }

        for table in &model.tables {
            push_doc(&mut content, table.desc.as_deref());
            content.push_str(&format!(
                "struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for field in &table.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "    ");
                content.push_str(&format!(
                    "    {} {};\n",
                    names.cpp_type(&field.code_type)?,
                    names.field_name(&field.name)?
                ));
            }
            for fragment in &table.fragments {
                content.push_str(&format!(
                    "    std::optional<{}> {};\n",
                    names.fragment_type_name(&table.name, &fragment.name)?,
                    names.field_name(&fragment.name)?
                ));
            }
            content.push_str("};\n\n");
            render_table(&mut content, table, &names)?;
        }

        content.push_str(
            "class IRuntimeDataLoader {\npublic:\n    virtual ~IRuntimeDataLoader() = default;\n",
        );
        content.push_str("    // Implementations should read exported table files such as DataFile, decode the payload envelope `{ meta, rows }`, and return the typed rows.\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    virtual std::vector<{}> Load{}Rows(const std::string& path) = 0;\n",
                names.row_type_name(&table.name)?,
                names.type_name(&table.name)?
            ));
        }
        content.push_str("};\n\n");

        content.push_str("class GameData {\npublic:\n");
        content.push_str("    static GameData Load(IRuntimeDataLoader& loader);\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    const {}& {}() const {{ return {}; }}\n",
                names.repository_type_name(&table.name)?,
                names.type_name(&table.name)?,
                names.member_name(&table.name)?
            ));
        }
        content.push_str("private:\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    {} {};\n",
                names.repository_type_name(&table.name)?,
                names.member_name(&table.name)?
            ));
        }
        content.push_str("};\n");
        if let Some(namespace) = namespace {
            content.push_str(&format!("\n}} // namespace {namespace}\n"));
        }

        Ok(vec![GeneratedFile {
            path: input.target.out_dir.join("generated.hpp"),
            kind: ArtifactKind::GeneratedCode,
            content,
        }])
    }
}

fn validate_generated_symbols(model: &CodeModel, names: &CppNames<'_>) -> Result<()> {
    let mut type_symbols = vec![GeneratedSymbol::new(
        "runtime interface",
        "IRuntimeDataLoader",
        "IRuntimeDataLoader",
    )];
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
        type_symbols.push(GeneratedSymbol::new(
            "table repository",
            format!("{}_table", table.name),
            names.repository_type_name(&table.name)?,
        ));
        for fragment in &table.fragments {
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

    let mut loader_symbols = vec![GeneratedSymbol::new(
        "method",
        "destructor",
        "~IRuntimeDataLoader",
    )];
    for table in &model.tables {
        loader_symbols.push(GeneratedSymbol::new(
            "loader method",
            &table.name,
            format!("Load{}Rows", names.type_name(&table.name)?),
        ));
    }
    ensure_unique_generated_symbols("cpp", "IRuntimeDataLoader methods", loader_symbols)
}

fn validate_named_symbols(
    language: &str,
    scope: &str,
    symbols: impl Iterator<Item = Result<GeneratedSymbol>>,
) -> Result<()> {
    ensure_unique_generated_symbols(language, scope, symbols.collect::<Result<Vec<_>>>()?)
}

fn reject_compound_indexes(model: &super::CodeModel) -> Result<()> {
    for table in &model.tables {
        for index in &table.indexes {
            if index.fields.len() > 1 {
                return Err(ConfigForgeError::Validation(format!(
                    "cpp codegen: table `{}` index `{}` uses compound keys, which are not supported yet",
                    table.name, index.name
                )));
            }
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
            names.cpp_index_storage(index)?,
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
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{} {}",
                names.cpp_type(&field.code_type)?,
                names.field_name(&field.name)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
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

    fn fragment_type_name(&self, table_name: &str, fragment_name: &str) -> Result<String> {
        self.type_name(&format!("{table_name}_{fragment_name}_fragment"))
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

    fn cpp_index_storage(&self, index: &CodeIndex) -> Result<String> {
        let key = self.cpp_key_type(index)?;
        if index.unique {
            Ok(format!("std::unordered_map<{key}, std::size_t>"))
        } else {
            Ok(format!(
                "std::unordered_map<{key}, std::vector<std::size_t>>"
            ))
        }
    }

    fn cpp_key_type(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.cpp_type(&index.fields[0].code_type)
        } else {
            self.type_name(&format!("{}_key", index.name))
        }
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
