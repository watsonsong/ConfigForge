// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{ConfigForgeError, Result};

use super::{
    CodeEnum, CodeExportInput, CodeExportLanguage, CodeFragmentPayload, CodeGenerator, CodeIndex,
    CodeModel, CodeStruct, CodeTable, CodeTableKind, CodeTypeRef, CodegenOptions, GeneratedFile,
    GeneratedSymbol, build_code_model, ensure_unique_generated_symbols,
    ir::require_non_empty_index_fields, naming, render_file_header,
};

pub struct TypeScriptCodeGenerator;

impl CodeGenerator for TypeScriptCodeGenerator {
    fn language(&self) -> CodeExportLanguage {
        CodeExportLanguage::TypeScript
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let typescript_options = input.target.options.typescript()?;
        let names = TypeScriptNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut content = render_file_header(input);
        content.push('\n');
        content
            .push_str("export const CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\" as const;\n\n");
        render_runtime_reader_interfaces(&mut content);
        render_runtime_reader_helpers(&mut content);

        for code_enum in &model.enums {
            push_doc(&mut content, code_enum.desc.as_deref());
            content.push_str(&format!(
                "export type {} = {};\n\n",
                names.type_name(&code_enum.name)?,
                code_enum
                    .values
                    .iter()
                    .map(|value| Ok(format!("{:?}", names.enum_value_name(&value.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(" | ")
            ));
            render_enum_reader_function(&mut content, code_enum, &names)?;
        }

        for code_struct in &model.structs {
            push_doc(&mut content, code_struct.desc.as_deref());
            content.push_str(&format!(
                "export interface {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                content.push_str(&format!(
                    "  {}{}: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&field.name)?,
                    names.ts_type(&field.code_type)?
                ));
            }
            content.push_str("}\n\n");
            render_struct_reader_function(&mut content, code_struct, &names)?;
        }

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut content, fragment.desc.as_deref());
                content.push_str(&format!(
                    "export interface {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                    content.push_str(&format!(
                        "  {}{}: {};\n",
                        readonly_prefix(typescript_options.readonly_fields),
                        names.field_name(&field.name)?,
                        names.ts_type(&field.code_type)?
                    ));
                }
                content.push_str("}\n\n");
                render_fragment_reader_function(&mut content, table, fragment, &names)?;
            }
        }

        for table in &model.tables {
            push_doc(&mut content, table.desc.as_deref());
            content.push_str(&format!(
                "export interface {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for field in &table.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                content.push_str(&format!(
                    "  {}{}: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&field.name)?,
                    names.ts_type(&field.code_type)?
                ));
            }
            for fragment in &table.fragments {
                content.push_str(&format!(
                    "  {}{}?: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?
                ));
            }
            content.push_str("}\n\n");
            render_row_reader_function(&mut content, table, &names)?;
            if table.kind == CodeTableKind::Collection {
                render_table(&mut content, table, &names)?;
            }
        }

        content.push_str("export class GameData {\n");
        for table in &model.tables {
            content.push_str(&format!(
                "  readonly {}: {};\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.push_str("\n  private constructor(args: {\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    {}: {};\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.push_str("  }) {\n");
        for table in &model.tables {
            content.push_str(&format!(
                "    this.{} = args.{};\n",
                names.field_name(&table.name)?,
                names.field_name(&table.name)?
            ));
        }
        content.push_str("  }\n\n");
        content.push_str("  static async load(loader: RuntimeDataLoader): Promise<GameData> {\n");
        content.push_str("    return new GameData({\n");
        for table in &model.tables {
            match table.kind {
                CodeTableKind::Collection => content.push_str(&format!(
                    "      {}: await {}.load(loader),\n",
                    names.field_name(&table.name)?,
                    names.repository_type_name(&table.name)?
                )),
                CodeTableKind::Object => content.push_str(&format!(
                    "      {}: {}(await loader.open({:?})),\n",
                    names.field_name(&table.name)?,
                    names.reader_function_name(&names.row_type_name(&table.name)?)?,
                    table.data_file
                )),
            }
        }
        content.push_str("    });\n");
        content.push_str("  }\n");
        content.push_str("}\n");

        Ok(vec![GeneratedFile {
            path: input.target.out_dir.join("generated.ts"),
            content,
        }])
    }
}

fn render_runtime_reader_interfaces(content: &mut String) {
    content.push_str(
        r#"export interface RuntimeDataReader {
  field(name: string): RuntimeDataReader | undefined;
  index(index: number): RuntimeDataReader;
  length(): number;
  asNumber(): number;
  asBool(): boolean;
  asString(): string;
  isNull(): boolean;
}

export interface RuntimeDataLoader {
  open(path: string): Promise<RuntimeDataReader> | RuntimeDataReader;
}

"#,
    );
}

fn render_runtime_reader_helpers(content: &mut String) {
    content.push_str(
        r#"function requiredField(reader: RuntimeDataReader, name: string): RuntimeDataReader {
  const value = reader.field(name);
  if (!value || value.isNull()) {
    throw new Error(`missing required field ${name}`);
  }
  return value;
}

function optionalField(reader: RuntimeDataReader, name: string): RuntimeDataReader | undefined {
  const value = reader.field(name);
  return value && !value.isNull() ? value : undefined;
}

function readRequiredNumber(reader: RuntimeDataReader, name: string): number {
  return requiredField(reader, name).asNumber();
}

function readRequiredBool(reader: RuntimeDataReader, name: string): boolean {
  return requiredField(reader, name).asBool();
}

function readRequiredString(reader: RuntimeDataReader, name: string): string {
  return requiredField(reader, name).asString();
}

"#,
    );
}

fn render_enum_reader_function(
    content: &mut String,
    code_enum: &CodeEnum,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_enum.name)?;
    let function_name = names.reader_function_name(&type_name)?;
    content.push_str(&format!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    content.push_str("  const value = reader.asString();\n");
    let values = code_enum
        .values
        .iter()
        .map(|value| {
            Ok(format!(
                "value === {:?}",
                names.enum_value_name(&value.name)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(" || ");
    content.push_str(&format!(
        "  if ({values}) {{ return value as {type_name}; }}\n"
    ));
    content.push_str(&format!(
        "  throw new Error(`unknown {} value ${{value}}`);\n",
        code_enum.name
    ));
    content.push_str("}\n\n");
    Ok(())
}

fn render_struct_reader_function(
    content: &mut String,
    code_struct: &CodeStruct,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.type_name(&code_struct.name)?;
    render_reader_function_body(content, &type_name, &code_struct.fields, None, names)
}

fn render_fragment_reader_function(
    content: &mut String,
    table: &CodeTable,
    fragment: &super::CodeFragment,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.fragment_payload_type_name(table, fragment)?;
    render_reader_function_body(content, &type_name, &fragment.fields, None, names)
}

fn render_row_reader_function(
    content: &mut String,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let type_name = names.row_type_name(&table.name)?;
    render_reader_function_body(content, &type_name, &table.fields, Some(table), names)
}

fn render_reader_function_body(
    content: &mut String,
    type_name: &str,
    fields: &[super::CodeField],
    table: Option<&CodeTable>,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let function_name = names.reader_function_name(type_name)?;
    content.push_str(&format!(
        "function {function_name}(reader: RuntimeDataReader): {type_name} {{\n"
    ));
    if let Some(table) = table {
        for fragment in &table.fragments {
            content.push_str(&format!(
                "  const {}Reader = optionalField(reader, {:?});\n",
                names.field_name(&fragment.name)?,
                fragment.name
            ));
        }
    }
    content.push_str("  return {\n");
    for field in fields {
        content.push_str(&format!(
            "    {}: {},\n",
            names.field_name(&field.name)?,
            names.ts_read_field_expr(field)?
        ));
    }
    if let Some(table) = table {
        for fragment in &table.fragments {
            let field_name = names.field_name(&fragment.name)?;
            let payload_type = names.fragment_payload_type_name(table, fragment)?;
            let reader_function = names.reader_function_name(&payload_type)?;
            content.push_str(&format!(
                "    {field_name}: {field_name}Reader ? {reader_function}({field_name}Reader) : undefined,\n"
            ));
        }
    }
    content.push_str("  };\n");
    content.push_str("}\n\n");
    Ok(())
}

fn validate_generated_symbols(model: &CodeModel, names: &TypeScriptNames<'_>) -> Result<()> {
    let mut type_symbols = vec![
        GeneratedSymbol::new("interface", "RuntimeDataLoader", "RuntimeDataLoader"),
        GeneratedSymbol::new("interface", "RuntimeDataReader", "RuntimeDataReader"),
        GeneratedSymbol::new("class", "GameData", "GameData"),
    ];
    for code_enum in &model.enums {
        type_symbols.push(GeneratedSymbol::new(
            "enum type",
            &code_enum.name,
            names.type_name(&code_enum.name)?,
        ));
    }
    for code_struct in &model.structs {
        type_symbols.push(GeneratedSymbol::new(
            "struct interface",
            &code_struct.name,
            names.type_name(&code_struct.name)?,
        ));
        validate_named_symbols(
            "typescript",
            &format!("interface `{}` fields", code_struct.name),
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
            "table row interface",
            format!("{}_row", table.name),
            names.row_type_name(&table.name)?,
        ));
        if table.kind == CodeTableKind::Collection {
            type_symbols.push(GeneratedSymbol::new(
                "table class",
                format!("{}_table", table.name),
                names.repository_type_name(&table.name)?,
            ));
        }
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::InlineFields) {
                type_symbols.push(GeneratedSymbol::new(
                    "fragment interface",
                    format!("{}_{}", table.name, fragment.name),
                    names.fragment_type_name(&table.name, &fragment.name)?,
                ));
                validate_named_symbols(
                    "typescript",
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
            "typescript",
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

        let mut member_symbols = vec![
            GeneratedSymbol::new("static field", "dataFile", "dataFile"),
            GeneratedSymbol::new("field", "rows_", "rows_"),
        ];
        for index in &table.indexes {
            member_symbols.push(GeneratedSymbol::new(
                "index storage",
                &index.name,
                names.index_storage_name(index)?,
            ));
        }
        ensure_unique_generated_symbols(
            "typescript",
            &format!("table `{}` class members", table.name),
            member_symbols,
        )?;

        let mut method_symbols = vec![
            GeneratedSymbol::new("method", "constructor", "constructor"),
            GeneratedSymbol::new("method", "load", "load"),
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
            "typescript",
            &format!("table `{}` class methods", table.name),
            method_symbols,
        )?;
    }
    ensure_unique_generated_symbols("typescript", "top-level exports", type_symbols)?;

    validate_named_symbols(
        "typescript",
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

fn render_table(
    content: &mut String,
    table: &CodeTable,
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    content.push_str(&format!(
        "export class {} {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str(&format!(
        "  static readonly dataFile = {:?};\n",
        table.data_file
    ));
    content.push_str(&format!(
        "  private readonly rows_: {}[];\n",
        names.row_type_name(&table.name)?
    ));
    for index in &table.indexes {
        content.push_str(&format!(
            "  private readonly {}: {};\n",
            names.index_storage_name(index)?,
            names.ts_index_storage(index, table)?
        ));
    }
    content.push('\n');
    content.push_str(&format!(
        "  private constructor(rows: {}[]) {{\n",
        names.row_type_name(&table.name)?
    ));
    content.push_str("    this.rows_ = rows;\n");
    for index in &table.indexes {
        content.push_str(&format!(
            "    this.{} = new Map();\n",
            names.index_storage_name(index)?
        ));
    }
    content.push_str("    for (const row of rows) {\n");
    for index in &table.indexes {
        let key = names.ts_key_expr(index)?;
        if index.unique {
            content.push_str(&format!(
                "      const key_{} = {};\n",
                names.index_storage_name(index)?,
                key
            ));
            content.push_str(&format!(
                "      if (this.{}.has(key_{})) {{ throw new Error(\"Duplicate unique index key for {}.{}\"); }}\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?,
                names.repository_type_name(&table.name)?,
                index.name
            ));
            content.push_str(&format!(
                "      this.{}.set(key_{}, row);\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?
            ));
        } else {
            content.push_str(&format!(
                "      const list_{} = this.{}.get({}) ?? [];\n",
                names.index_storage_name(index)?,
                names.index_storage_name(index)?,
                key
            ));
            content.push_str(&format!(
                "      list_{}.push(row);\n",
                names.index_storage_name(index)?
            ));
            content.push_str(&format!(
                "      this.{}.set({}, list_{});\n",
                names.index_storage_name(index)?,
                key,
                names.index_storage_name(index)?
            ));
        }
    }
    content.push_str("    }\n");
    content.push_str("  }\n\n");
    content.push_str(&format!(
        "  static async load(loader: RuntimeDataLoader): Promise<{}> {{\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str("    const root = await loader.open(this.dataFile);\n");
    content.push_str("    const rowsReader = requiredField(root, \"rows\");\n");
    content.push_str(&format!(
        "    const rows: {}[] = [];\n",
        names.row_type_name(&table.name)?
    ));
    content.push_str("    for (let rowIndex = 0; rowIndex < rowsReader.length(); rowIndex++) {\n");
    content.push_str(&format!(
        "      rows.push({}(rowsReader.index(rowIndex)));\n",
        names.reader_function_name(&names.row_type_name(&table.name)?)?
    ));
    content.push_str("    }\n");
    content.push_str(&format!(
        "    return new {}(rows ?? []);\n",
        names.repository_type_name(&table.name)?
    ));
    content.push_str("  }\n\n");
    content.push_str(&format!(
        "  rows(): readonly {}[] {{ return this.rows_; }}\n\n",
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
    names: &TypeScriptNames<'_>,
) -> Result<()> {
    let params = index
        .fields
        .iter()
        .map(|field| {
            Ok(format!(
                "{}: {}",
                names.field_name(&field.name)?,
                names.ts_type(&field.code_type)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    let key = names.ts_lookup_key(index)?;
    if index.unique {
        content.push_str(&format!(
            "  {}({}): {} | undefined {{ return this.{}.get({}); }}\n\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?,
            names.index_storage_name(index)?,
            key
        ));
    } else {
        content.push_str(&format!(
            "  {}({}): readonly {}[] {{ return this.{}.get({}) ?? []; }}\n\n",
            names.index_function_name(index)?,
            params,
            names.row_type_name(&table.name)?,
            names.index_storage_name(index)?,
            key
        ));
    }
    Ok(())
}

struct TypeScriptNames<'a> {
    options: &'a CodegenOptions,
}

impl<'a> TypeScriptNames<'a> {
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

    fn enum_value_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.enum_value)
    }

    fn field_name(&self, name: &str) -> Result<String> {
        self.identifier(name, self.options.naming.field)
    }

    fn index_storage_name(&self, index: &CodeIndex) -> Result<String> {
        self.identifier(
            &format!("{}_index", self.index_function_name(index)?),
            self.options.naming.field,
        )
    }

    fn index_function_name(&self, index: &CodeIndex) -> Result<String> {
        let prefix = if index.unique { "get_by" } else { "find_by" };
        let suffix = index.name.trim_start_matches("by_");
        self.identifier(&format!("{prefix}_{suffix}"), self.options.naming.function)
    }

    fn ts_type(&self, code_type: &CodeTypeRef) -> Result<String> {
        Ok(match code_type {
            CodeTypeRef::Int | CodeTypeRef::Float => "number".to_owned(),
            CodeTypeRef::Bool => "boolean".to_owned(),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => "string".to_owned(),
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => self.type_name(name)?,
        })
    }

    fn reader_function_name(&self, type_name: &str) -> Result<String> {
        if self.options.naming.function == super::CaseStyle::CamelCase {
            return naming::resolve_keyword_conflict(
                format!("read{type_name}"),
                self.options.keyword_conflict,
                naming::is_ts_keyword,
                |_| None,
            )
            .map_err(|message| {
                ConfigForgeError::Validation(format!("typescript codegen: {message}"))
            });
        }
        self.identifier(&format!("read_{type_name}"), self.options.naming.function)
    }

    fn ts_read_field_expr(&self, field: &super::CodeField) -> Result<String> {
        let source_name = format!("{:?}", field.name);
        Ok(match &field.code_type {
            CodeTypeRef::Int | CodeTypeRef::Float => {
                format!("readRequiredNumber(reader, {source_name})")
            }
            CodeTypeRef::Bool => format!("readRequiredBool(reader, {source_name})"),
            CodeTypeRef::String | CodeTypeRef::Text | CodeTypeRef::Asset => {
                format!("readRequiredString(reader, {source_name})")
            }
            CodeTypeRef::Enum { name } | CodeTypeRef::Struct { name } => format!(
                "{}(requiredField(reader, {source_name}))",
                self.reader_function_name(&self.type_name(name)?)?
            ),
        })
    }

    fn ts_index_storage(&self, index: &CodeIndex, table: &CodeTable) -> Result<String> {
        let key_type = if index.fields.len() == 1 {
            self.ts_type(&index.fields[0].code_type)?
        } else {
            "string".to_owned()
        };
        if index.unique {
            Ok(format!(
                "Map<{key_type}, {}>",
                self.row_type_name(&table.name)?
            ))
        } else {
            Ok(format!(
                "Map<{key_type}, {}[]>",
                self.row_type_name(&table.name)?
            ))
        }
    }

    fn ts_key_expr(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            Ok(format!("row.{}", self.field_name(&index.fields[0].name)?))
        } else {
            Ok(format!(
                "JSON.stringify([{}])",
                index
                    .fields
                    .iter()
                    .map(|field| Ok(format!("row.{}", self.field_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))
        }
    }

    fn ts_lookup_key(&self, index: &CodeIndex) -> Result<String> {
        if index.fields.len() == 1 {
            self.field_name(&index.fields[0].name)
        } else {
            Ok(format!(
                "JSON.stringify([{}])",
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
            naming::is_ts_keyword,
            |_| None,
        )
        .map_err(|message| ConfigForgeError::Validation(format!("typescript codegen: {message}")))
    }
}

fn push_doc(content: &mut String, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut String, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        content.push_str(indent);
        content.push_str("/**\n");
        for line in desc.lines() {
            content.push_str(indent);
            content.push_str(" * ");
            content.push_str(line);
            content.push('\n');
        }
        content.push_str(indent);
        content.push_str(" */\n");
    }
}

fn readonly_prefix(readonly_fields: bool) -> &'static str {
    if readonly_fields { "readonly " } else { "" }
}
