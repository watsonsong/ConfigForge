use crate::{ConfigForgeError, Result};

use super::{
    CodeEnum, CodeExportInput, CodeExportLanguage, CodeFragmentPayload, CodeGenerator, CodeIndex,
    CodeModel, CodeStruct, CodeTable, CodeTableKind, CodeTypeRef, CodegenOptions, GeneratedFile,
    GeneratedSymbol, build_code_model, ensure_unique_generated_symbols,
    ir::require_non_empty_index_fields, naming, render_file_header,
};

pub struct RustCodeGenerator;

impl CodeGenerator for RustCodeGenerator {
    fn language(&self) -> CodeExportLanguage {
        CodeExportLanguage::Rust
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let rust_options = input.target.options.rust()?;
        let names = RustNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut content = render_file_header(input);
        content.push_str("use std::collections::HashMap;\n\n");
        content.push_str("pub const CONFIG_FORGE_READER_CONTRACT_VERSION: &str = \"1.0\";\n\n");
        render_runtime_reader_traits(&mut content);
        render_runtime_reader_helpers(&mut content);

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
            render_enum_reader_impl(&mut content, code_enum, &names)?;
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
                content.push_str(&format!(
                    "pub struct {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
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
            for fragment in &table.fragments {
                if rust_options.derive_serde {
                    content.push_str(&format!("    #[serde(rename = {:?})]\n", fragment.name));
                }
                content.push_str(&format!(
                    "    pub {}: Option<{}>,\n",
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?
                ));
            }
            content.push_str("}\n\n");
            render_row_reader_impl(&mut content, table, &names)?;
            content.push_str(&format!("impl {} {{\n", names.row_type_name(&table.name)?));
            for fragment in &table.fragments {
                content.push_str(&format!(
                    "    pub fn has_{}(&self) -> bool {{ self.{}.is_some() }}\n",
                    names.field_name(&fragment.name)?,
                    names.field_name(&fragment.name)?
                ));
                content.push_str(&format!(
                    "    pub fn {}(&self) -> Option<&{}> {{ self.{}.as_ref() }}\n",
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?,
                    names.field_name(&fragment.name)?
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
                names.table_data_type_name(table)?
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
                names.table_data_type_name(table)?
            ));
        }
        content.push_str("        })\n");
        content.push_str("    }\n");
        content.push_str("}\n");

        Ok(vec![GeneratedFile {
            path: input.target.out_dir.join("generated.rs"),
            content,
        }])
    }
}

fn render_runtime_reader_traits(content: &mut String) {
    content.push_str(
        r#"pub trait RuntimeDataReader: Clone {
    fn field(&self, name: &str) -> Result<Option<Self>, String>;
    fn index(&self, index: usize) -> Result<Self, String>;
    fn len(&self) -> Result<usize, String>;
    fn as_i64(&self) -> Result<i64, String>;
    fn as_f64(&self) -> Result<f64, String>;
    fn as_bool(&self) -> Result<bool, String>;
    fn as_string(&self) -> Result<String, String>;
    fn is_null(&self) -> bool;
}

pub trait RuntimeDataLoader {
    type Reader: RuntimeDataReader;
    fn open(&self, path: &str) -> Result<Self::Reader, String>;
}

"#,
    );
}

fn render_runtime_reader_helpers(content: &mut String) {
    content.push_str(
        r#"fn required_field<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<R, String> {
    match reader.field(name)? {
        Some(value) if !value.is_null() => Ok(value),
        _ => Err(format!("missing required field `{name}`")),
    }
}

fn optional_field<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<Option<R>, String> {
    Ok(reader.field(name)?.filter(|value| !value.is_null()))
}

fn read_required_i64<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<i64, String> {
    required_field(reader, name)?.as_i64()
}

fn read_required_f64<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<f64, String> {
    required_field(reader, name)?.as_f64()
}

fn read_required_bool<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<bool, String> {
    required_field(reader, name)?.as_bool()
}

fn read_required_string<R: RuntimeDataReader>(reader: &R, name: &str) -> Result<String, String> {
    required_field(reader, name)?.as_string()
}

"#,
    );
}

fn render_enum_reader_impl(
    content: &mut String,
    code_enum: &CodeEnum,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(&format!("impl {} {{\n", names.type_name(&code_enum.name)?));
    content.push_str(
        "    fn read_from_reader<R: RuntimeDataReader>(reader: &R) -> Result<Self, String> {\n",
    );
    content.push_str("        match reader.as_string()?.as_str() {\n");
    for value in &code_enum.values {
        content.push_str(&format!(
            "            {:?} => Ok(Self::{}),\n",
            value.name,
            names.enum_variant_name(&value.name)?
        ));
    }
    content.push_str(&format!(
        "            other => Err(format!(\"unknown {} value `{{other}}`\")),\n",
        code_enum.name
    ));
    content.push_str("        }\n");
    content.push_str("    }\n");
    content.push_str("}\n\n");
    Ok(())
}

fn render_struct_reader_impl(
    content: &mut String,
    code_struct: &CodeStruct,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(&format!(
        "impl {} {{\n",
        names.type_name(&code_struct.name)?
    ));
    render_read_from_reader_body(content, &code_struct.fields, None, &code_struct.name, names)?;
    content.push_str("}\n\n");
    Ok(())
}

fn render_fragment_reader_impl(
    content: &mut String,
    table: &CodeTable,
    fragment: &super::CodeFragment,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(&format!(
        "impl {} {{\n",
        names.fragment_type_name(&table.name, &fragment.name)?
    ));
    render_read_from_reader_body(content, &fragment.fields, None, &fragment.name, names)?;
    content.push_str("}\n\n");
    Ok(())
}

fn render_row_reader_impl(
    content: &mut String,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(&format!("impl {} {{\n", names.row_type_name(&table.name)?));
    render_read_from_reader_body(content, &table.fields, Some(table), &table.name, names)?;
    content.push_str("}\n\n");
    Ok(())
}

fn render_read_from_reader_body(
    content: &mut String,
    fields: &[super::CodeField],
    table: Option<&CodeTable>,
    type_name: &str,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(
        "    fn read_from_reader<R: RuntimeDataReader>(reader: &R) -> Result<Self, String> {\n",
    );
    content.push_str("        Ok(Self {\n");
    for field in fields {
        content.push_str(&format!(
            "            {}: {},\n",
            names.field_name(&field.name)?,
            names.rust_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            content.push_str(&format!(
                "            {field_name}: match optional_field(reader, {:?})? {{ Some(value) => Some({payload_type}::read_from_reader(&value)?), None => None }},\n",
                fragment.name
            ));
        }
    }
    content.push_str("        })\n");
    content.push_str("    }\n");
    if fields.is_empty()
        && table
            .map(|table| table.fragments.is_empty())
            .unwrap_or(true)
    {
        content.push_str(&format!("    // `{type_name}` has no runtime fields.\n"));
    }
    Ok(())
}

fn validate_generated_symbols(model: &CodeModel, names: &RustNames<'_>) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new(
            "runtime interface",
            "RuntimeDataReader",
            "RuntimeDataReader",
        ),
        GeneratedSymbol::new(
            "runtime interface",
            "RuntimeDataLoader",
            "RuntimeDataLoader",
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
            "rust",
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

        validate_named_symbols(
            "rust",
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

        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "rust",
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

        if table.kind == CodeTableKind::Object {
            continue;
        }

        let mut storage_symbols = vec![GeneratedSymbol::new("field", "rows", "rows")];
        for index in &table.indexes {
            storage_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "rust",
            &format!("table `{}` repository fields", table.name),
            storage_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "load", "load"),
            GeneratedSymbol::new("method", "from_rows", "from_rows"),
            GeneratedSymbol::new("method", "rows", "rows"),
        ];
        for index in &table.indexes {
            method_symbols.push(GeneratedSymbol::new(
                "index function",
                &index.name,
                names.index_function_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "rust",
            &format!("table `{}` methods", table.name),
            method_symbols,
        )?;
    }
    ensure_unique_generated_symbols("rust", "top-level types", type_symbols)?;

    validate_named_symbols(
        "rust",
        "GameData fields",
        model.tables.iter().map(|table| {
            Ok(GeneratedSymbol::new(
                "table field",
                &table.name,
                names.field_name(&table.name)?,
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

fn render_table(content: &mut String, table: &CodeTable, names: &RustNames<'_>) -> Result<()> {
    if table.kind == CodeTableKind::Object {
        return render_object_table(content, table, names);
    }

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
    content.push_str("        let root = loader.open(Self::DATA_FILE)?;\n");
    content.push_str("        let rows_reader = required_field(&root, \"rows\")?;\n");
    content.push_str("        let mut rows = Vec::new();\n");
    content.push_str("        for row_index in 0..rows_reader.len()? {\n");
    content.push_str(&format!(
        "            rows.push({}::read_from_reader(&rows_reader.index(row_index)?)?);\n",
        names.row_type_name(&table.name)?
    ));
    content.push_str("        }\n");
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

fn render_object_table(
    content: &mut String,
    table: &CodeTable,
    names: &RustNames<'_>,
) -> Result<()> {
    content.push_str(&format!("impl {} {{\n", names.row_type_name(&table.name)?));
    content.push_str(&format!(
        "    pub const DATA_FILE: &'static str = {:?};\n\n",
        table.data_file
    ));
    content
        .push_str("    pub fn load(loader: &impl RuntimeDataLoader) -> Result<Self, String> {\n");
    content.push_str("        let root = loader.open(Self::DATA_FILE)?;\n");
    content.push_str("        Self::read_from_reader(&root)\n");
    content.push_str("    }\n");
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

    fn rust_read_field_expr(&self, field: &super::CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        Ok(match &field.code_type {
            CodeTypeRef::Int => format!("read_required_i64(reader, {source_name})?"),
            CodeTypeRef::Float => format!("read_required_f64(reader, {source_name})?"),
            CodeTypeRef::Bool => format!("read_required_bool(reader, {source_name})?"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("read_required_string(reader, {source_name})?")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => format!(
                "{}::read_from_reader(&required_field(reader, {source_name})?)?",
                self.type_name(name)?
            ),
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
