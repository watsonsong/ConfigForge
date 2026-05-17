use crate::{ConfigForgeError, Result};

use super::{
    build_code_model, ir::require_non_empty_index_fields, naming, render_file_header, ArtifactKind,
    CodeExportInput, CodeExportLanguage, CodeGenerator, CodeIndex, CodeTable, CodeTypeRef,
    CodegenOptions, GeneratedFile,
};

pub struct RustCodeGenerator;

impl CodeGenerator for RustCodeGenerator {
    fn language(&self) -> CodeExportLanguage {
        CodeExportLanguage::Rust
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let rust_options = input.target.options.rust()?;
        let names = RustNames::new(&input.target.options);
        let mut content = render_file_header(input);
        content.push_str("use std::collections::HashMap;\n\n");
        content.push_str("pub trait RuntimeDataLoader {\n");
        content.push_str("    // Implementations should read exported table files such as DATA_FILE, decode the payload envelope `{ meta, rows }`, and return the typed `rows`.\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    fn load_{}_rows(&self, path: &str) -> Result<Vec<{}>, String>;\n",
                names.field_name(&table.name)?,
                names.row_type_name(&table.name)?
            ));
        }
        content.push_str("}\n\n");

        for code_enum in &model.enums {
            push_doc(&mut content, code_enum.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq, Eq, Hash",
                rust_options.derive_serde,
            );
            content.push_str(&format!(
                "pub enum {} {{\n",
                names.type_name(&code_enum.name)?
            ));
            for value in &code_enum.values {
                push_doc(&mut content, value.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.push_str(&format!("    #[serde(rename = {:?})]\n", value.name));
                }
                content.push_str(&format!("    {},\n", names.enum_variant_name(&value.name)?));
            }
            content.push_str("}\n\n");
        }

        for code_struct in &model.structs {
            push_doc(&mut content, code_struct.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq",
                rust_options.derive_serde,
            );
            content.push_str(&format!(
                "pub struct {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc(&mut content, field.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.push_str(&format!("    #[serde(rename = {:?})]\n", field.name));
                }
                content.push_str(&format!(
                    "    pub {}: {},\n",
                    names.field_name(&field.name)?,
                    names.rust_type(&field.code_type)?
                ));
            }
            content.push_str("}\n\n");
        }

        for table in &model.tables {
            push_doc(&mut content, table.desc.as_deref(), "/// ");
            push_derive(
                &mut content,
                "Debug, Clone, PartialEq",
                rust_options.derive_serde,
            );
            content.push_str(&format!(
                "pub struct {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for field in &table.fields {
                push_doc(&mut content, field.desc.as_deref(), "    /// ");
                if rust_options.derive_serde {
                    content.push_str(&format!("    #[serde(rename = {:?})]\n", field.name));
                }
                content.push_str(&format!(
                    "    pub {}: {},\n",
                    names.field_name(&field.name)?,
                    names.rust_type(&field.code_type)?
                ));
            }
            content.push_str("}\n\n");
            render_table(&mut content, table, &names)?;
        }

        content.push_str("pub struct GameData {\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    pub {}: {},\n",
                names.field_name(&table.name)?,
                names.repository_type_name(&table.name)?
            ));
        }
        content.push_str("}\n\n");
        content.push_str("impl GameData {\n");
        content.push_str(
            "    pub fn load(loader: &impl RuntimeDataLoader) -> Result<Self, String> {\n",
        );
        content.push_str("        Ok(Self {\n");
        for table in &model.tables {
            content.push_str(&format!(
                "            {}: {}::load(loader)?,\n",
                names.field_name(&table.name)?,
                names.repository_type_name(&table.name)?
            ));
        }
        content.push_str("        })\n");
        content.push_str("    }\n");
        content.push_str("}\n");

        Ok(vec![GeneratedFile {
            path: input.target.out_dir.join("generated.rs"),
            kind: ArtifactKind::GeneratedCode,
            content,
        }])
    }
}

fn render_table(content: &mut String, table: &CodeTable, names: &RustNames<'_>) -> Result<()> {
    content.push_str(&format!(
        "pub struct {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "    rows: Vec<{}>,\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.push_str(&format!(
            "    {}: {},\n",
            names.index_storage_name(index)?,
            names.rust_index_storage(index)?
        ));
    }
    content.push_str("}\n\n");

    content.push_str(&format!(
        "impl {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "    pub const DATA_FILE: &'static str = {:?};\n\n",
        table.data_file
    ));
    content
        .push_str("    pub fn load(loader: &impl RuntimeDataLoader) -> Result<Self, String> {\n");
    content.push_str(&format!(
        "        let rows = loader.load_{}_rows(Self::DATA_FILE)?;\n",
        names.field_name(&table.name)?
    ));
    content.push_str("        Self::from_rows(rows)\n");
    content.push_str("    }\n\n");
    content.push_str(&format!(
        "    pub fn from_rows(rows: Vec<{}>) -> Result<Self, String> {{\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.push_str(&format!(
            "        let mut {} = HashMap::new();\n",
            names.index_storage_name(index)?
        ));
    }
    content.push_str("        for (row_index, row) in rows.iter().enumerate() {\n");
    for index in &table.indexes {
        let key = names.rust_key_expr(index)?;
        if index.unique {
            content.push_str(&format!(
                "            if {}.insert({}, row_index).is_some() {{ return Err(format!(\"duplicate unique index {}\")); }}\n",
                names.index_storage_name(index)?,
                key,
                index.name
            ));
        } else {
            content.push_str(&format!(
                "            {}.entry({}).or_insert_with(Vec::new).push(row_index);\n",
                names.index_storage_name(index)?,
                key
            ));
        }
    }
    content.push_str("        }\n");
    content.push_str("        Ok(Self {\n");
    content.push_str("            rows,\n");
    for index in &table.indexes {
        content.push_str(&format!(
            "            {},\n",
            names.index_storage_name(index)?
        ));
    }
    content.push_str("        })\n");
    content.push_str("    }\n\n");
    content.push_str(&format!(
        "    pub fn rows(&self) -> &[{}] {{ &self.rows }}\n\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        render_index_function(content, table, index, names)?;
    }
    content.push_str("}\n\n");
    Ok(())
}

fn render_index_function(
    content: &mut String,
    table: &CodeTable,
    index: &CodeIndex,
    names: &RustNames<'_>,
) -> Result<()> {
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{}: {}",
                names.field_name(&field.name)?,
                names.rust_type(&field.code_type)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let key = names.rust_lookup_key(index)?;
    if index.unique {
        content.push_str(&format!(
            "    pub fn {}(&self, {}) -> Option<&{}> {{\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?
        ));
        content.push_str(&format!(
            "        self.{}.get(&{}).and_then(|index| self.rows.get(*index))\n",
            names.index_storage_name(index)?,
            key
        ));
        content.push_str("    }\n\n");
    } else {
        content.push_str(&format!(
            "    pub fn {}(&self, {}) -> Vec<&{}> {{\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?
        ));
        content.push_str(&format!(
            "        self.{}.get(&{}).map(|indexes| indexes.iter().filter_map(|index| self.rows.get(*index)).collect()).unwrap_or_default()\n",
            names.index_storage_name(index)?, key
        ));
        content.push_str("    }\n\n");
    }
    Ok(())
}

struct RustNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> RustNames<'a> {
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

    fn enum_variant_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.field_name(&index.name)
    }

    fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    fn rust_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int => "i64".to_owned(),
            CodeTypeRef::Float => "f64".to_owned(),
            CodeTypeRef::Bool => "bool".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "String".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
        })
    }

    fn rust_index_storage(&self, index: &CodeIndex) -> Result<String> {
        let key_type = self.rust_key_type(index)?;
        if index.unique {
            Ok(format!("HashMap<{key_type}, usize>"))
        } else {
            Ok(format!("HashMap<{key_type}, Vec<usize>>"))
        }
    }

    fn rust_key_type(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.rust_type(&index.fields[0].code_type)
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| self.rust_type(&field.code_type))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    fn rust_key_expr(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!(
                "row.{}.clone()",
                self.field_name(&index.fields[0].name)?
            ))
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("row.{}.clone()", self.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    fn rust_lookup_key(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            Ok(format!(
                "({})",
                index
                    .fields
                    .iter()
                    .map(|field| self.field_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    fn identifier(&self, value: &str, style: super::CaseStyle) -> Result<String> {
        naming::make_identifier(
            value,
            style,
            self.options.keyword_conflict,
            naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .map_err(|message| ConfigForgeError::Validation(format!("rust codegen: {message}")))
    }
}

fn push_doc(content: &mut String, desc: Option<&str>, prefix: &str) {
    if let Some(desc) = desc {
        for line in desc.lines() {
            content.push_str(prefix);
            content.push_str(line);
            content.push('\n');
        }
    }
}

fn push_derive(content: &mut String, base_derives: &str, derive_serde: bool) {
    if derive_serde {
        content.push_str(&format!(
            "#[derive({base_derives}, serde::Serialize, serde::Deserialize)]\n"
        ));
    } else {
        content.push_str(&format!("#[derive({base_derives})]\n"));
    }
}
