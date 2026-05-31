// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::Result;

use super::{
    CaseStyle, CodeExportInput, CodeField, CodeFragmentPayload, CodeGenerator, CodeTableKind,
    CodeWriter, CodeWriterConfig, GeneratedFile, I18nFunctionReturn, I18nMode, I18nOptions,
    NamingConfig, build_code_model, ir::require_non_empty_index_fields,
};

mod reader;
mod repository;
mod runtime;
mod symbols;
mod types;

use reader::{
    render_enum_reader_function, render_fragment_reader_function, render_row_reader_function,
    render_struct_reader_function,
};
use repository::render_table;
use repository::{
    render_game_data_link_all, render_row_relation_cache_members, render_row_relation_members,
    render_row_relation_methods,
};
use runtime::{render_runtime_reader_helpers, render_runtime_reader_interfaces};
use symbols::validate_generated_symbols;
use types::TypeScriptNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "typescript";

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TypeScriptCodegenOptions {
    #[serde(default)]
    pub readonly_fields: bool,
}

pub struct TypeScriptCodeGenerator;

impl CodeGenerator for TypeScriptCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn language_aliases(&self) -> &'static [&'static str] {
        &["ts"]
    }

    fn default_module_name(&self) -> &'static str {
        "configForgeGenerated"
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::Preserve,
            field: CaseStyle::CamelCase,
            function: CaseStyle::CamelCase,
            constant: CaseStyle::CamelCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(2)
    }

    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<GeneratedFile>> {
        let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
        require_non_empty_index_fields(&model)?;
        let typescript_options = input
            .target
            .options
            .language_options::<TypeScriptCodegenOptions>(LANGUAGE)?;
        let names = TypeScriptNames::new(&input.target.options);
        validate_generated_symbols(&model, &names)?;
        let mut content = CodeWriter::new(input.target.options.formatting);
        self.write_file_header(&mut content, input);
        content.newline();
        render_ts_i18n_import(&mut content, &input.target.options.i18n);
        content.raw("export const CONFIG_FORGE_READER_CONTRACT_VERSION = \"1.0\" as const;\n\n");
        render_runtime_reader_interfaces(&mut content);
        render_runtime_reader_helpers(&mut content);
        render_ts_i18n_prelude(&mut content, &input.target.options.i18n);
        if model.tables.iter().any(|table| {
            table
                .foreign_keys
                .iter()
                .any(|foreign_key| foreign_key.linked_field.is_some())
        }) {
            content.raw(concat!(
                "const LINK_UNRESOLVED = -1;\n",
                "const LINK_EMPTY = -2;\n",
                "type LinkCache = number;\n",
                "type LinkListCache = number[] | undefined;\n\n",
            ));
        }

        for code_enum in &model.enums {
            push_doc(&mut content, code_enum.desc.as_deref());
            content.raw_fmt(format_args!(
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
            content.raw_fmt(format_args!(
                "export interface {} {{\n",
                names.type_name(&code_struct.name)?
            ));
            for field in &code_struct.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                content.raw_fmt(format_args!(
                    "  {}{}{}: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&field.name)?,
                    names.ts_optional_property_suffix(field),
                    names.ts_type(&field.code_type)?
                ));
            }
            content.raw("}\n\n");
            render_struct_reader_function(&mut content, code_struct, &names)?;
            render_ts_text_helpers(
                &mut content,
                &code_struct.name,
                &code_struct.name,
                &code_struct.fields,
                &names,
                &input.target.options.i18n,
            )?;
        }

        for table in &model.tables {
            for fragment in &table.fragments {
                if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                    continue;
                }
                push_doc(&mut content, fragment.desc.as_deref());
                content.raw_fmt(format_args!(
                    "export interface {} {{\n",
                    names.fragment_type_name(&table.name, &fragment.name)?
                ));
                for field in &fragment.fields {
                    push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                    content.raw_fmt(format_args!(
                        "  {}{}{}: {};\n",
                        readonly_prefix(typescript_options.readonly_fields),
                        names.field_name(&field.name)?,
                        names.ts_optional_property_suffix(field),
                        names.ts_type(&field.code_type)?
                    ));
                }
                content.raw("}\n\n");
                render_fragment_reader_function(&mut content, table, fragment, &names)?;
                render_ts_text_helpers(
                    &mut content,
                    &format!("{}_{}", table.name, fragment.name),
                    &format!("{}.{}", table.name, fragment.name),
                    &fragment.fields,
                    &names,
                    &input.target.options.i18n,
                )?;
            }
        }

        for table in &model.tables {
            push_doc(&mut content, table.desc.as_deref());
            content.raw_fmt(format_args!(
                "export interface {} {{\n",
                names.row_type_name(&table.name)?
            ));
            for field in &table.fields {
                push_doc_indented(&mut content, field.desc.as_deref(), "  ");
                content.raw_fmt(format_args!(
                    "  {}{}{}: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&field.name)?,
                    names.ts_optional_property_suffix(field),
                    names.ts_type(&field.code_type)?
                ));
            }
            for fragment in &table.fragments {
                content.raw_fmt(format_args!(
                    "  {}{}?: {};\n",
                    readonly_prefix(typescript_options.readonly_fields),
                    names.field_name(&fragment.name)?,
                    names.fragment_payload_type_name(table, fragment)?
                ));
            }
            render_row_relation_members(&mut content, &model, table, &names)?;
            render_row_relation_cache_members(&mut content, table, &names)?;
            content.raw("}\n\n");
            render_row_reader_function(&mut content, table, &names)?;
            render_ts_text_helpers(
                &mut content,
                &table.name,
                &table.name,
                &table.fields,
                &names,
                &input.target.options.i18n,
            )?;
            if table.kind == CodeTableKind::Collection {
                render_table(&mut content, table, &names)?;
            }
        }

        content.raw("export class GameData {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "  readonly {}: {};\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("\n  private constructor(args: {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    {}: {};\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("  }) {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    this.{} = args.{};\n",
                names.field_name(&table.name)?,
                names.field_name(&table.name)?
            ));
        }
        content.raw("  }\n\n");
        content.raw("  static async load(loader: RuntimeDataLoader): Promise<GameData> {\n");
        content.raw("    return new GameData({\n");
        for table in &model.tables {
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "      {}: await {}.load(loader),\n",
                    names.field_name(&table.name)?,
                    names.repository_type_name(&table.name)?
                )),
                CodeTableKind::Object => {
                    content.raw_fmt(format_args!(
                        "      {}: await Promise.resolve(loader.open({:?})).then(root => {{\n",
                        names.field_name(&table.name)?,
                        table.data_file
                    ));
                    if let Some(revision) = table.schema_revision {
                        content.raw_fmt(format_args!(
                            concat!(
                                "        const schemaRevision = root.schemaRevision?.();\n",
                                "        if (schemaRevision !== undefined && BigInt(schemaRevision) !== {revision}n) {{\n",
                                "          throw new Error(`schema revision mismatch for {}`);\n",
                                "        }}\n",
                            ),
                            table.data_file,
                            revision = revision
                        ));
                    }
                    content.raw_fmt(format_args!(
                        concat!(
                            "        const value = {}(root);\n",
                            "        root.ensureExhausted?.();\n",
                            "        return value;\n",
                            "      }}),\n",
                        ),
                        names.reader_function_name(&names.row_type_name(&table.name)?)?
                    ));
                }
            }
        }
        content.raw("    });\n");
        content.raw("  }\n");
        content.raw("\n  static loadSync(loader: RuntimeDataLoader): GameData {\n");
        content.raw("    return new GameData({\n");
        for table in &model.tables {
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "      {}: {}.loadSync(loader),\n",
                    names.field_name(&table.name)?,
                    names.repository_type_name(&table.name)?
                )),
                CodeTableKind::Object => {
                    content.raw_fmt(format_args!(
                        "      {}: (() => {{\n        const root = openRuntimeDataSync(loader, {:?});\n",
                        names.field_name(&table.name)?,
                        table.data_file
                    ));
                    if let Some(revision) = table.schema_revision {
                        content.raw_fmt(format_args!(
                            concat!(
                                "        const schemaRevision = root.schemaRevision?.();\n",
                                "        if (schemaRevision !== undefined && BigInt(schemaRevision) !== {revision}n) {{\n",
                                "          throw new Error(`schema revision mismatch for {}`);\n",
                                "        }}\n",
                            ),
                            table.data_file,
                            revision = revision
                        ));
                    }
                    content.raw_fmt(format_args!(
                        concat!(
                            "        const value = {}(root);\n",
                            "        root.ensureExhausted?.();\n",
                            "        return value;\n",
                            "      }})(),\n",
                        ),
                        names.reader_function_name(&names.row_type_name(&table.name)?)?
                    ));
                }
            }
        }
        content.raw("    });\n");
        content.raw("  }\n");
        content.raw("\n  static loadLazy(loader: RuntimeDataLoader): LazyGameData { return new LazyGameData(loader); }\n");
        content.newline();
        render_game_data_link_all(&mut content, &model, &names)?;
        content.raw("}\n");

        content.raw("\nexport class LazyGameData {\n");
        content.raw("  private readonly loader: RuntimeDataLoader;\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "  private {}Promise?: Promise<{}>;\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
            content.raw_fmt(format_args!(
                "  private {}Value?: {};\n",
                names.field_name(&table.name)?,
                names.table_data_type_name(table)?
            ));
        }
        content.raw("\n  constructor(loader: RuntimeDataLoader) {\n");
        content.raw("    this.loader = loader;\n");
        content.raw("  }\n\n");
        content.raw("  static loadLazy(loader: RuntimeDataLoader): LazyGameData { return new LazyGameData(loader); }\n\n");
        for table in &model.tables {
            let field_name = names.field_name(&table.name)?;
            let data_type = names.table_data_type_name(table)?;
            content.raw_fmt(format_args!(
                "  {}(): Promise<{}> {{\n",
                field_name, data_type
            ));
            content.raw_fmt(format_args!("    if (!this.{field_name}Promise) {{\n"));
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "      this.{field_name}Promise = {}.load(this.loader).then((value) => {{ this.{field_name}Value = value; return value; }}).catch((error) => {{ this.{field_name}Promise = undefined; throw error; }});\n",
                    names.repository_type_name(&table.name)?
                )),
                CodeTableKind::Object => {
                    content.raw_fmt(format_args!(
                        "      this.{field_name}Promise = Promise.resolve(this.loader.open({:?})).then((root) => {{\n",
                        table.data_file
                    ));
                    if let Some(revision) = table.schema_revision {
                        content.raw_fmt(format_args!(
                            concat!(
                                "        const schemaRevision = root.schemaRevision?.();\n",
                                "        if (schemaRevision !== undefined && BigInt(schemaRevision) !== {revision}n) {{\n",
                                "          throw new Error(`schema revision mismatch for {}`);\n",
                                "        }}\n",
                            ),
                            table.data_file,
                            revision = revision
                        ));
                    }
                    content.raw_fmt(format_args!(
                        concat!(
                            "        const value = {}(root);\n",
                            "        root.ensureExhausted?.();\n",
                            "        this.{field_name}Value = value;\n",
                            "        return value;\n",
                            "      }}).catch((error) => {{ this.{field_name}Promise = undefined; throw error; }});\n",
                        ),
                        names.reader_function_name(&names.row_type_name(&table.name)?)?,
                        field_name = field_name
                    ));
                }
            }
            content.raw("    }\n");
            content.raw_fmt(format_args!("    return this.{field_name}Promise;\n"));
            content.raw("  }\n\n");
            content.raw_fmt(format_args!("  {}Sync(): {} {{\n", field_name, data_type));
            content.raw_fmt(format_args!(
                "    if (this.{field_name}Value) {{ return this.{field_name}Value; }}\n"
            ));
            content.raw_fmt(format_args!(
                "    if (this.{field_name}Promise) {{ throw new Error({:?}); }}\n",
                format!(
                    "cannot synchronously access lazy table `{}` while async loading is in progress",
                    table.name
                )
            ));
            match table.kind {
                CodeTableKind::Collection => content.raw_fmt(format_args!(
                    "    const value = {}.loadSync(this.loader);\n",
                    names.repository_type_name(&table.name)?
                )),
                CodeTableKind::Object => {
                    content.raw_fmt(format_args!(
                        "    const root = openRuntimeDataSync(this.loader, {:?});\n",
                        table.data_file
                    ));
                    if let Some(revision) = table.schema_revision {
                        content.raw_fmt(format_args!(
                            concat!(
                                "    const schemaRevision = root.schemaRevision?.();\n",
                                "    if (schemaRevision !== undefined && BigInt(schemaRevision) !== {revision}n) {{\n",
                                "      throw new Error(`schema revision mismatch for {}`);\n",
                                "    }}\n",
                            ),
                            table.data_file,
                            revision = revision
                        ));
                    }
                    content.raw_fmt(format_args!(
                        "    const value = {}(root);\n",
                        names.reader_function_name(&names.row_type_name(&table.name)?)?
                    ));
                    content.raw("    root.ensureExhausted?.();\n");
                }
            }
            content.raw_fmt(format_args!("    this.{field_name}Value = value;\n"));
            content.raw_fmt(format_args!(
                "    this.{field_name}Promise = Promise.resolve(value);\n"
            ));
            content.raw("    return value;\n");
            content.raw("  }\n\n");
        }
        content.raw("  async preloadAll(): Promise<void> {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    await this.{}();\n",
                names.field_name(&table.name)?
            ));
        }
        content.raw("  }\n\n");
        content.raw("  async linkAll(): Promise<void> {\n");
        content.raw("    await this.preloadAll();\n");
        content.raw("    const data = {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "      {}: await this.{}(),\n",
                names.field_name(&table.name)?,
                names.field_name(&table.name)?
            ));
        }
        content.raw("    } as GameData;\n");
        for table in &model.tables {
            if table
                .foreign_keys
                .iter()
                .any(|foreign_key| foreign_key.linked_field.is_some())
            {
                content.raw_fmt(format_args!(
                    "    data.{}.link(data);\n",
                    names.field_name(&table.name)?
                ));
            }
        }
        content.raw("  }\n");
        content.raw("\n  preloadAllSync(): void {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "    this.{}Sync();\n",
                names.field_name(&table.name)?
            ));
        }
        content.raw("  }\n\n");
        content.raw("  linkAllSync(): void {\n");
        content.raw("    this.preloadAllSync();\n");
        content.raw("    const data = {\n");
        for table in &model.tables {
            content.raw_fmt(format_args!(
                "      {}: this.{}Sync(),\n",
                names.field_name(&table.name)?,
                names.field_name(&table.name)?
            ));
        }
        content.raw("    } as GameData;\n");
        for table in &model.tables {
            if table
                .foreign_keys
                .iter()
                .any(|foreign_key| foreign_key.linked_field.is_some())
            {
                content.raw_fmt(format_args!(
                    "    data.{}.link(data);\n",
                    names.field_name(&table.name)?
                ));
            }
        }
        content.raw("  }\n");
        content.raw("}\n");

        for table in &model.tables {
            render_row_relation_methods(&mut content, &model, table, &names)?;
        }

        Ok(vec![GeneratedFile {
            path: input
                .target
                .out_dir
                .join(input.target.single_file_name("generated", "ts")),
            content: content.finish(),
        }])
    }
}

fn render_ts_text_helpers(
    content: &mut CodeWriter,
    owner_name: &str,
    context_prefix: &str,
    fields: &[CodeField],
    names: &TypeScriptNames<'_>,
    i18n: &I18nOptions,
) -> Result<()> {
    for field in fields
        .iter()
        .filter(|field| matches!(field.code_type, super::CodeTypeRef::Text))
    {
        let function_name = names.text_function_name(owner_name, &field.name)?;
        let field_name = names.field_name(&field.name)?;
        let context = format!("{context_prefix}.{}", field.name);
        match i18n.mode {
            I18nMode::Generic if field.required => {
                content.raw_fmt(format_args!(
                    concat!(
                        "export function {function_name}(value: {{ {field_name}: string }}, translator: TextTranslator): string {{\n",
                        "  return translator({context:?}, value.{field_name}) ?? value.{field_name};\n",
                        "}}\n\n",
                    ),
                    function_name = function_name,
                    context = context,
                    field_name = field_name,
                ));
            }
            I18nMode::Generic => {
                content.raw_fmt(format_args!(
                    concat!(
                        "export function {function_name}(value: {{ {field_name}?: string }}, translator: TextTranslator): string | undefined {{\n",
                        "  const msgid = value.{field_name};\n",
                        "  return msgid === undefined ? undefined : (translator({context:?}, msgid) ?? msgid);\n",
                        "}}\n\n",
                    ),
                    function_name = function_name,
                    context = context,
                    field_name = field_name,
                ));
            }
            I18nMode::GlobalFunction if field.required => {
                content.raw_fmt(format_args!(
                    concat!(
                        "export function {function_name}(value: {{ {field_name}: string }}): string {{\n",
                        "  return {expr};\n",
                        "}}\n\n",
                    ),
                    function_name = function_name,
                    field_name = field_name,
                    expr = ts_global_i18n_expr(i18n, &context, &format!("value.{field_name}"), &format!("value.{field_name}")),
                ));
            }
            I18nMode::GlobalFunction => {
                content.raw_fmt(format_args!(
                    concat!(
                        "export function {function_name}(value: {{ {field_name}?: string }}): string | undefined {{\n",
                        "  const msgid = value.{field_name};\n",
                        "  return msgid === undefined ? undefined : {expr};\n",
                        "}}\n\n",
                    ),
                    function_name = function_name,
                    field_name = field_name,
                    expr = ts_global_i18n_expr(i18n, &context, "msgid", "msgid"),
                ));
            }
        }
    }
    Ok(())
}

fn render_ts_i18n_import(content: &mut CodeWriter, i18n: &I18nOptions) {
    if !matches!(i18n.mode, I18nMode::GlobalFunction) {
        return;
    }
    if let Some(import) = i18n.typescript_import.as_deref() {
        content.raw(import);
        if !import.ends_with(';') {
            content.raw(";");
        }
        content.raw("\n\n");
    }
}

fn render_ts_i18n_prelude(content: &mut CodeWriter, i18n: &I18nOptions) {
    if matches!(i18n.mode, I18nMode::Generic) {
        content.raw("export type TextTranslator = (msgctxt: string | undefined, msgid: string) => string | undefined;\n\n");
    }
}

fn ts_global_i18n_expr(
    i18n: &I18nOptions,
    context: &str,
    msgid_expr: &str,
    fallback_expr: &str,
) -> String {
    let function = i18n.function.as_deref().expect("global i18n function");
    match i18n.returns {
        I18nFunctionReturn::String => format!("{function}({context:?}, {msgid_expr})"),
        I18nFunctionReturn::Optional => {
            format!("({function}({context:?}, {msgid_expr}) ?? {fallback_expr})")
        }
    }
}

fn push_doc(content: &mut CodeWriter, desc: Option<&str>) {
    push_doc_indented(content, desc, "");
}

fn push_doc_indented(content: &mut CodeWriter, desc: Option<&str>, indent: &str) {
    if let Some(desc) = desc {
        content.raw(indent);
        content.raw("/**\n");
        for line in desc.lines() {
            content.raw(indent);
            content.raw(" * ");
            content.raw(line);
            content.newline();
        }
        content.raw(indent);
        content.raw(" */\n");
    }
}

fn readonly_prefix(readonly_fields: bool) -> &'static str {
    if readonly_fields { "readonly " } else { "" }
}
