// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::Result;

use super::{
    CaseStyle, CodeExportInput, CodeFragmentPayload, CodeGenerator, CodeWriter, CodeWriterConfig,
    GeneratedFile, NamingConfig, build_code_model, ir::require_non_empty_index_fields,
};

mod reader;
mod repository;
mod runtime;
mod symbols;
mod types;

use reader::{
    render_enum_reader_impl, render_fragment_reader_impl, render_row_reader_impl,
    render_struct_reader_impl,
};
use repository::render_table;
use runtime::{render_runtime_reader_helpers, render_runtime_reader_imports};
use symbols::validate_generated_symbols;
use types::RustNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "rust";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RustCodegenOptions {
    #[serde(default)]
    pub derive_serde: bool,
}

pub struct RustCodeGenerator;

impl CodeGenerator for RustCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::SnakeCase,
            function: CaseStyle::SnakeCase,
            constant: CaseStyle::ScreamingSnakeCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let rust_options = input
            .target
            .options
            .language_options::<RustCodegenOptions>(LANGUAGE)?;
        let names = RustNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut content = CodeWriter::new(input.target.options.formatting);
        self.write_file_header(&mut content, input);
        content.raw("use std::collections::HashMap;\n\n");
        render_runtime_reader_imports(&mut content);
        render_runtime_reader_helpers(&mut content);

        for code_enum in &model.enums {
            push_doc(&mut content, code_enum.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq, Eq, Hash",
                rust_options.derive_serde,
            );
            content.raw_fmt(format_args!(
                "pub enum {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc(&mut content, value.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.raw_fmt(format_args!("    #[serde(rename = {:?})]\n", value.name));
                }
                content.raw_fmt(format_args!(
                    "    {},\n",
                    names.enum_variant_name(&value.name)?
                ));
            }
            content.raw("}\n\n");
            render_enum_reader_impl(&mut content, code_enum, &names)?;
        }

        for code_struct in &model.structs {
            push_doc(&mut content, code_struct.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq",
                rust_options.derive_serde,
            );
            content.raw_fmt(format_args!(
                "pub struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc(&mut content, field.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.raw_fmt(format_args!("    #[serde(rename = {:?})]\n", field.name));
                }
                content.raw_fmt(format_args!(
                    "    pub {}: {},\n",
                    names.field_name(&field.name)?,
                    names.rust_field_type(field)?
                ));
            }
            content.raw("}\n\n");
            render_struct_reader_impl(&mut content, code_struct, &names)?;
        }

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut content, fragment.desc.as_deref(), "/// ");
                push_derive(
                    &mut content,
                    "Debug, Clone, PartialEq",
                    rust_options.derive_serde,
                );
                content.raw_fmt(format_args!(
                    "pub struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc(&mut content, field.desc.as_deref(), "    /// ");
                    if rust_options.derive_serde {
                        content.raw_fmt(format_args!("    #[serde(rename = {:?})]\n", field.name));
                    }
                    content.raw_fmt(format_args!(
                        "    pub {}: {},\n",
                        names.field_name(&field.name)?,
                        names.rust_field_type(field)?
                    ));
                }
                content.raw("}\n\n");
                render_fragment_reader_impl(&mut content, table, fragment, &names)?;
            }
        }

        for table in &model.tables {
            push_doc(&mut content, table.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq",
                rust_options.derive_serde,
            );
            content.raw_fmt(format_args!(
                "pub struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for field in &table.fields {
                push_doc(&mut content, field.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.raw_fmt(format_args!("    #[serde(rename = {:?})]\n", field.name));
                }
                content.raw_fmt(format_args!(
                    "    pub {}: {},\n",
                    names.field_name(&field.name)?,
                    names.rust_field_type(field)?
                ));
            }
            for fragment in &table.fragments {
                if rust_options.derive_serde {
                    content.raw_fmt(format_args!("    #[serde(rename = {:?})]\n", fragment.name));
                }
                content.raw_fmt(format_args!(
                    "    pub {}: Option<{}>,\n",
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?
                ));
            }
            content.raw("}\n\n");
            render_row_reader_impl(&mut content, table, &names)?;
            content.raw_fmt(format_args!(
                "impl {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for fragment in &table.fragments {
                content.raw_fmt(format_args!(
                    "    pub fn has_{}(&self) -> bool {{ self.{}.is_some() }}\n",
                    names.field_name(&fragment.name)?,
                    names.field_name(&fragment.name)?
                ));
                content.raw_fmt(format_args!(
                    "    pub fn {}(&self) -> Option<&{}> {{ self.{}.as_ref() }}\n",
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?,
                    names.field_name(&fragment.name)?
                ));
            }
            content.raw("}\n\n");
            render_table(&mut content, table, &names)?;
        }

        content.raw("pub struct GameData {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    pub {}: {},\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("}\n\n");
        content.raw("impl GameData {\n");
        content
            .raw("    pub fn load(loader: &impl RuntimeDataLoader) -> RuntimeDataResult<Self> {\n");
        content.raw("        Ok(Self {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "            {}: {}::load(loader)?,\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("        })\n");
        content.raw("    }\n");
        content.raw("}\n");

        Ok(vec![GeneratedFile {
            path: input.target.out_dir.join("generated.rs"),
            content: content.finish(),
        }])
    }
}

fn push_doc(content: &mut CodeWriter, desc: Option<&str>, prefix: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.raw(prefix);
            content.raw(line);
            content.newline();
        }
    }
}

fn push_derive(content: &mut CodeWriter, base_derives: &str, derive_serde: bool) {
    if derive_serde {
        content.raw_fmt(format_args!(
            "#[derive({base_derives}, serde::Serialize, serde::Deserialize)]\n"
        ));
    } else {
        content.raw_fmt(format_args!("#[derive({base_derives})]\n"));
    }
}
