// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::Result;

use super::{
    CaseStyle, CodeExportInput, CodeFragmentPayload, CodeGenerator, CodeModel, CodeTableKind,
    CodeWriter, CodeWriterConfig, GeneratedFile, NamingConfig, build_code_model,
    ir::require_non_empty_index_fields,
};

mod reader;
mod repository;
mod runtime;
mod symbols;
mod types;

use reader::{
    render_enum_reader_source, render_fragment_reader_source, render_row_reader_source,
    render_struct_reader_source,
};
use repository::{
    render_compound_index_key_structs, render_game_data_load_source, render_table,
    render_table_source,
};
use runtime::{render_reader_helpers_source, render_runtime_interfaces};
use symbols::validate_generated_symbols;
use types::CppNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "cpp";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct CppCodegenOptions {
    #[serde(default)]
    pub namespace: Option<String>,
}

pub struct CppCodeGenerator;

impl CodeGenerator for CppCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn language_aliases(&self) -> &'static [&'static str] {
        &["c++"]
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::SnakeCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let cpp_options = input
            .target
            .options
            .language_options::<CppCodegenOptions>(LANGUAGE)?;
        let names = CppNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut header = CodeWriter::new(input.target.options.formatting);
        self.write_file_header(&mut header, input);
        header.raw("#pragma once\n\n");
        header.raw(concat!(
            "#include <cstdint>\n",
            "#include <cstddef>\n",
            "#include <map>\n",
            "#include <memory>\n",
            "#include <optional>\n",
            "#include <stdexcept>\n",
            "#include <string>\n",
            "#include <tuple>\n",
            "#include <type_traits>\n",
            "#include <unordered_map>\n",
            "#include <utility>\n",
            "#include <vector>\n\n",
        ));
        let namespace = cpp_options
            .namespace
            .as_ref()
            .or(input.target.namespace.as_ref());
        let namespace = namespace.map(String::as_str);
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
        }
        render_runtime_interfaces(&mut header);

        for code_enum in &model.enums {
            push_doc(&mut header, code_enum.desc.as_deref());
            header.raw_fmt(format_args!(
                "enum class {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc_indented(&mut header, value.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {},\n",
                    names.enum_variant_name(&value.name)?
                ));
            }
            header.raw("};\n\n");
        }

        for code_struct in &model.structs {
            push_doc(&mut header, code_struct.desc.as_deref());
            header.raw_fmt(format_args!(
                "struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            header.raw("};\n\n");
        }

        render_compound_index_key_structs(&mut header, &model, &names)?;

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut header, fragment.desc.as_deref());
                header.raw_fmt(format_args!(
                    "struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                    header.raw_fmt(format_args!(
                        "    {} {};\n",
                        names.cpp_field_type(field)?,
                        names.field_name(&field.name)?
                    ));
                }
                header.raw("};\n\n");
            }
        }

        for table in &model.tables {
            push_doc(&mut header, table.desc.as_deref());
            header.raw_fmt(format_args!(
                "struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            if table.kind == CodeTableKind::Object {
                header.raw_fmt(format_args!(
                    "    static constexpr const char* DataFile = {:?};\n",
                    table.data_file
                ));
                if let Some(revision) = table.schema_revision {
                    header.raw_fmt(format_args!(
                        "    static constexpr std::uint64_t ExpectedSchemaRevision = {revision}ULL;\n"
                    ));
                }
            }
            for field in &table.fields {
                push_doc_indented(&mut header, field.desc.as_deref(), "    ");
                header.raw_fmt(format_args!(
                    "    {} {};\n",
                    names.cpp_field_type(field)?,
                    names.field_name(&field.name)?
                ));
            }
            for fragment in &table.fragments {
                header.raw_fmt(format_args!(
                    "    std::optional<{}> {};\n",
                    names.fragment_payload_type_name(table, fragment)?,
                    names.field_name(&fragment.name)?
                ));
            }
            header.raw("};\n\n");
            if table.kind == CodeTableKind::Collection {
                render_table(&mut header, table, &names)?;
            }
        }

        header.raw("class GameData {\npublic:\n");
        header.raw("    static GameData Load(IRuntimeDataLoader& loader);\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    const {}& {}() const {{ return {}; }}\n",
                names.table_data_type_name(table)?,
                names.type_name(&table.name)?,
                names.member_name(&table.name)?
            ));
        }
        header.raw("private:\n");
        for table in &model.tables {
            header.raw_fmt(format_args!(
                "    {} {};\n",
                names.table_data_type_name(table)?,
                names.member_name(&table.name)?
            ));
        }
        header.raw("};\n");
        if let Some(namespace) = namespace {
            header.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
        }

        let header_file_name = input.target.single_file_name("generated", "hpp");
        let source_file_name = format!("{}.cpp", input.target.file_stem("generated"));
        let source = render_source_file(self, input, &model, namespace, &names, &header_file_name)?;

        Ok(vec![
            GeneratedFile {
                path: input.target.out_dir.join(header_file_name),
                content: header.finish(),
            },
            GeneratedFile {
                path: input.target.out_dir.join(source_file_name),
                content: source,
            },
        ])
    }
}

fn render_source_file(
    generator: &dyn CodeGenerator,
    input: &CodeExportInput<'_>,
    model: &CodeModel,
    namespace: Option<&str>,
    names: &CppNames<'_>,
    header_file_name: &str,
) -> Result<String> {
    let mut content = CodeWriter::new(input.target.options.formatting);
    generator.write_file_header(&mut content, input);
    content.raw_fmt(format_args!("#include {:?}\n\n", header_file_name));
    content.raw("#include <stdexcept>\n\n");
    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("namespace {namespace} {{\n\n"));
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
    content.raw("} // namespace\n\n");

    for table in &model.tables {
        if table.kind == CodeTableKind::Collection {
            render_table_source(&mut content, table, names)?;
        }
    }
    render_game_data_load_source(&mut content, model, names)?;

    if let Some(namespace) = namespace {
        content.raw_fmt(format_args!("\n}} // namespace {namespace}\n"));
    }
    Ok(content.finish())
}

fn push_doc(content: &mut CodeWriter, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut CodeWriter, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.raw(indent);
            content.raw("// ");
            content.raw(line);
            content.newline();
        }
    }
}
