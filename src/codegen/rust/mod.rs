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
use repository::{render_game_data_link_methods, render_row_relation_methods, render_table};
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
        content.raw("use std::{cell::{Cell, OnceCell, RefCell}, collections::HashMap};\n\n");
        render_runtime_reader_imports(&mut content);
        render_runtime_reader_helpers(&mut content);
        content.raw(concat!(
            "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
            "enum LinkCache {\n",
            "    Unresolved,\n",
            "    Empty,\n",
            "    RowIndex(usize),\n",
            "}\n\n",
            "impl Default for LinkCache {\n",
            "    fn default() -> Self { Self::Unresolved }\n",
            "}\n\n",
            "#[derive(Debug, Clone, PartialEq, Eq, Default)]\n",
            "struct LinkListCache {\n",
            "    row_indexes: Option<Vec<usize>>,\n",
            "}\n\n",
        ));

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
                    super::ir::CodeRelationCardinality::Many => "RefCell<LinkListCache>",
                    _ => "Cell<LinkCache>",
                };
                if rust_options.derive_serde {
                    content.raw("    #[serde(skip, default)]\n");
                }
                content.raw_fmt(format_args!(
                    "    {}: {},\n",
                    names.relation_cache_field_name(foreign_key)?,
                    cache_type
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
            render_row_relation_methods(&mut content, &model, table, &names)?;
            content.raw("}\n\n");
            render_table(&mut content, table, &names)?;
        }

        content.raw("pub trait GameDataAccess {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    fn {}(&self) -> RuntimeDataResult<&{}>;\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("}\n\n");

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
        render_game_data_link_methods(&mut content, &model, &names)?;
        content.raw("}\n");

        content.raw("\nimpl GameDataAccess for GameData {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    fn {}(&self) -> RuntimeDataResult<&{}> {{ Ok(&self.{}) }}\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?,
                names.field_name(&table.name)?
            ));
        }
        content.raw("}\n\n");

        content.raw("pub struct LazyGameData<L: RuntimeDataLoader> {\n");
        content.raw("    loader: L,\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    {}: OnceCell<{}>,\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("}\n\n");
        content.raw("impl<L: RuntimeDataLoader> LazyGameData<L> {\n");
        content.raw("    pub fn new(loader: L) -> Self {\n");
        content.raw("        Self {\n");
        content.raw("            loader,\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "            {}: OnceCell::new(),\n",
                names.field_name(&table.name)?
            ));
        }
        content.raw("        }\n");
        content.raw("    }\n\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    pub fn {}(&self) -> RuntimeDataResult<&{}> {{ <Self as GameDataAccess>::{}(self) }}\n\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?,
                names.field_name(&table.name)?
            ));
        }
        content.raw("    pub fn preload_all(&self) -> RuntimeDataResult<()> {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "        let _ = self.{}()?;\n",
                names.field_name(&table.name)?
            ));
        }
        content.raw("        Ok(())\n");
        content.raw("    }\n");
        content.raw("\n    pub fn link_all(&self) -> RuntimeDataResult<()> {\n");
        content.raw("        self.preload_all()?;\n");
        for table in &model.tables {
            if table
                .foreign_keys
                .iter()
                .any(|foreign_key| foreign_key.linked_field.is_some())
            {
                content.raw_fmt(format_args!(
                    "        self.{}()?.link(self)?;\n",
                    names.field_name(&table.name)?
                ));
            }
        }
        content.raw("        Ok(())\n");
        content.raw("    }\n");
        content.raw("}\n\n");

        content.raw("impl<L: RuntimeDataLoader> GameDataAccess for LazyGameData<L> {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                concat!(
                    "    fn {}(&self) -> RuntimeDataResult<&{}> {{\n",
                    "        if self.{}.get().is_none() {{\n",
                    "            let value = {}::load(&self.loader)?;\n",
                    "            let _ = self.{}.set(value);\n",
                    "        }}\n",
                    "        self.{}.get().ok_or_else(|| RuntimeDataError::decode({:?}.to_owned()))\n",
                    "    }}\n",
                ),
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?,
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?,
                names.field_name(&table.name)?,
                names.field_name(&table.name)?,
                format!("lazy table `{}` was not initialized", table.name)
            ));
        }
        content.raw("}\n");

        Ok(vec![GeneratedFile {
            path: input
                .target
                .out_dir
                .join(input.target.single_file_name("generated", "rs")),
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
