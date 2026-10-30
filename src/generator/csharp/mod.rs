// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactSink,
    code_artifact::runtime_interface::{
        REQUIRED_RUNTIME_READER_CONTRACT, RUNTIME_READER_CONTRACT_VERSION,
    },
    diagnostic::code::CF_GENERATOR_INVALID_OPTION,
};

use super::{
    ArtifactGenerator, CaseStyle, CheckedStreamingCodeWriter, CodeExportInput, CodeField,
    CodeFragmentPayload, CodeGenerator, CodeModel, CodeTableKind, CodeWriterConfig, CodeWriterLike,
    GeneratorCommonInput, GeneratorRequirements, I18nFunctionReturn, I18nMode, I18nOptions,
    NamingConfig, ReaderShapePlan, RuntimeInterfaceEmit, RuntimeInterfaceMode, build_code_model,
    ir::CodeRelationCardinality, ir::require_non_empty_index_fields,
};

mod symbols;
mod types;

use symbols::{validate_csharp_identifier, validate_csharp_namespace, validate_generated_symbols};
use types::CSharpNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "csharp";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CSharpCodegenOptions {
    #[serde(default)]
    pub profile: CSharpProfile,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default = "default_nullable")]
    pub nullable: bool,
    #[serde(default = "default_access_modifier")]
    pub access_modifier: String,
    #[serde(default)]
    pub class_name: Option<String>,
    #[serde(default)]
    pub lazy_class_name: Option<String>,
    #[serde(default = "default_emit_partial")]
    pub emit_partial: bool,
    #[serde(default)]
    pub runtime_interface: RuntimeInterfaceMode,
    #[serde(default)]
    pub runtime_interface_emit: RuntimeInterfaceEmit,
    #[serde(default)]
    pub runtime_namespace: Option<String>,
    #[serde(default)]
    pub emit_runtime_adapters: bool,
    #[serde(default)]
    pub type_mappings: CSharpTypeMappings,
    #[serde(default)]
    pub using: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct CSharpTypeMappings {
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub vector2: Option<String>,
    #[serde(default)]
    pub vector3: Option<String>,
    #[serde(default)]
    pub vector4: Option<String>,
    #[serde(default)]
    pub vector2_int: Option<String>,
    #[serde(default)]
    pub vector3_int: Option<String>,
    #[serde(default)]
    pub vector4_int: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CSharpProfile {
    #[default]
    Dotnet,
    Unity,
    Godot,
}

impl Default for CSharpCodegenOptions {
    fn default() -> Self {
        Self {
            profile: CSharpProfile::default(),
            namespace: None,
            nullable: default_nullable(),
            access_modifier: default_access_modifier(),
            class_name: None,
            lazy_class_name: None,
            emit_partial: default_emit_partial(),
            runtime_interface: RuntimeInterfaceMode::Inline,
            runtime_interface_emit: RuntimeInterfaceEmit::Copy,
            runtime_namespace: None,
            emit_runtime_adapters: false,
            type_mappings: CSharpTypeMappings::default(),
            using: Vec::new(),
        }
    }
}

fn default_nullable() -> bool {
    true
}

fn default_access_modifier() -> String {
    "public".to_owned()
}

fn default_emit_partial() -> bool {
    true
}

pub(crate) struct CSharpCodeGenerator;

impl CodeGenerator for CSharpCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn default_module_name(&self) -> &'static str {
        "ConfigForge.Generated"
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::PascalCase,
            field: CaseStyle::PascalCase,
            function: CaseStyle::PascalCase,
            constant: CaseStyle::PascalCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<super::GeneratedFile>> {
        let mut content = super::CodeWriter::new(input.target.options.formatting);
        render_csharp_code(self, &mut content, input)?;
        Ok(vec![super::GeneratedFile {
            path: csharp_output_path(input),
            content: content.finish(),
        }])
    }

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()> {
        let output_path = csharp_output_path(input);
        let stream = sink.create(output_path.clone())?;
        let mut content = CheckedStreamingCodeWriter::new(stream, input.target.options.formatting);
        render_csharp_code(self, &mut content, input)?;
        let stream = content
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: output_path,
                source,
            })?;
        stream.finish().map(|_| ())
    }
}

impl ArtifactGenerator for CSharpCodeGenerator {
    fn type_id(&self) -> &'static str {
        LANGUAGE
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["cs"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_only()
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<std::path::PathBuf>> {
        let input = input.code_export_input(self)?;
        Ok(vec![csharp_output_path(&input)])
    }

    fn generate_to_sink(
        &self,
        input: &GeneratorCommonInput<'_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        super::generate_code_artifact_to_sink(self, input, sink)
    }
}

fn csharp_output_path(input: &CodeExportInput<'_>) -> std::path::PathBuf {
    input
        .target
        .out_dir
        .join(input.target.single_file_name("generated", "cs"))
}

fn write_file_header(
    generator: &dyn CodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
) {
    if let Some(template) = &input.target.options.file_header {
        let project = &input.project;
        let mut output = template.to_owned();
        output = output.replace("{{project.name}}", project.name.as_deref().unwrap_or(""));
        output = output.replace("{{project.desc}}", project.desc.as_deref().unwrap_or(""));
        output = output.replace(
            "{{project.copyright}}",
            project.copyright.as_deref().unwrap_or(""),
        );
        output = output.replace("{{datetime}}", &input.generated_at);
        if !output.ends_with('\n') {
            output.push('\n');
        }
        content.raw(output);
        return;
    }

    let prefix = generator.line_comment_prefix();
    content.line(format!(
        "{prefix} Copyright (c) 2026 ConfigForge contributors"
    ));
    content.line(format!("{prefix} SPDX-License-Identifier: Apache-2.0"));
    content.blank_line();
    content.line(format!("{prefix} Generated by config-forge. Do not edit."));
}

fn render_csharp_code(
    generator: &CSharpCodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
) -> Result<()> {
    let options = input
        .target
        .options
        .language_options::<CSharpCodegenOptions>(LANGUAGE)?;
    validate_runtime_interface_options(&options)?;
    let names = CSharpNames::new(input, &options)?;
    let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
    require_non_empty_index_fields(&model)?;
    validate_generated_symbols(&model, &names)?;

    write_file_header(generator, content, input);
    content.newline();
    if options.nullable {
        content.raw("#nullable enable\n\n");
    }
    content.raw("using System;\nusing System.Collections.Generic;\n");
    render_custom_csharp_usings(content, &options.using);
    render_csharp_i18n_using(content, &input.target.options.i18n);
    content.raw("\n");
    content.raw_fmt(format_args!("namespace {}\n{{\n", names.namespace()));
    render_csharp_i18n_prelude(content, &input.target.options.i18n);
    render_runtime_contract(content, &model.enums, &names)?;
    if options.emit_runtime_adapters {
        render_standard_runtime_adapters(content);
    }
    let partial = if options.emit_partial { " partial" } else { "" };
    let class_modifier = format!("{}{}", names.access_modifier(), partial);

    for code_enum in &model.enums {
        content.raw_fmt(format_args!(
            "    {} enum {}\n    {{\n",
            names.access_modifier(),
            names.type_name(&code_enum.name)?
        ));
        for value in &code_enum.values {
            content.raw_fmt(format_args!(
                "        {},\n",
                names.enum_value_name(&value.name)?
            ));
        }
        content.raw("    }\n\n");
    }

    for code_struct in &model.structs {
        render_csharp_class(
            content,
            &class_modifier,
            &names.type_name(&code_struct.name)?,
            &code_struct.fields,
            &names,
            &input.target.options.i18n,
            &code_struct.name,
        )?;
        render_read_method(
            content,
            &names.type_name(&code_struct.name)?,
            &code_struct.fields,
            &names,
        )?;
    }

    for table in &model.tables {
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                continue;
            }
            render_csharp_class(
                content,
                &class_modifier,
                &names.fragment_type_name(&table.name, &fragment.name)?,
                &fragment.fields,
                &names,
                &input.target.options.i18n,
                &format!("{}.{}", table.name, fragment.name),
            )?;
            render_read_method(
                content,
                &names.fragment_type_name(&table.name, &fragment.name)?,
                &fragment.fields,
                &names,
            )?;
        }
        render_csharp_row_class(
            content,
            names.access_modifier(),
            partial,
            &names.row_type_name(&table.name)?,
            table,
            &names,
            &input.target.options.i18n,
        )?;
        render_row_relation_methods(content, table, &model, &names)?;
        render_row_read_method(content, &names.row_type_name(&table.name)?, table, &names)?;
        if table.kind == CodeTableKind::Collection {
            for index in table.indexes.iter().filter(|index| index.fields.len() > 1) {
                render_compound_key_type(
                    content,
                    &table.name,
                    index,
                    names.access_modifier(),
                    &names,
                )?;
            }
            render_table_class(content, names.access_modifier(), partial, table, &names)?;
        }
    }

    render_game_data_access_interface(content, &model, names.access_modifier(), &names)?;

    content.raw_fmt(format_args!(
        "    {}{} class {} : IGameDataAccess\n    {{\n",
        names.access_modifier(),
        partial,
        names.game_data_class_name()
    ));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "        public {} {} {{ get; }}\n",
            match table.kind {
                CodeTableKind::Object => names.row_type_name(&table.name)?,
                CodeTableKind::Collection => names.repository_type_name(&table.name)?,
            },
            names.property_name(&table.name)?
        ));
    }
    content.raw("\n        public void LinkAll()\n        {\n");
    for table in model.tables.iter().filter(|table| {
        table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
    }) {
        if table.kind == CodeTableKind::Collection {
            content.raw_fmt(format_args!(
                "            {}.Link(this);\n",
                names.property_name(&table.name)?
            ));
        }
    }
    content.raw("        }\n");
    content.raw("\n        private GameData(\n");
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        content.raw_fmt(format_args!(
            "            {} {}{}\n",
            match table.kind {
                CodeTableKind::Object => names.row_type_name(&table.name)?,
                CodeTableKind::Collection => names.repository_type_name(&table.name)?,
            },
            names.local_name(&table.name)?,
            comma
        ));
    }
    content.raw("        )\n        {\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "            {} = {};\n",
            names.property_name(&table.name)?,
            names.local_name(&table.name)?
        ));
    }
    content.raw("        }\n\n");
    content.raw_fmt(format_args!(
        "        internal static {} FromLoaded(\n",
        names.game_data_class_name()
    ));
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        content.raw_fmt(format_args!(
            "            {} {}{}\n",
            csharp_table_data_type(table, &names)?,
            names.local_name(&table.name)?,
            comma
        ));
    }
    content.raw_fmt(format_args!(
        "        )\n        {{\n            var data = new {}(\n",
        names.game_data_class_name()
    ));
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        content.raw_fmt(format_args!(
            "                {}{}\n",
            names.local_name(&table.name)?,
            comma
        ));
    }
    content.raw(
        "            );\n            data.LinkAll();\n            return data;\n        }\n\n",
    );
    content.raw_fmt(format_args!(
        "        public static {} Load(IConfigForgeRuntimeDataLoader loader)\n        {{\n            return FromLoaded(\n",
        names.game_data_class_name()
    ));
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        match table.kind {
            CodeTableKind::Object => content.raw_fmt(format_args!(
                "                {}.Load(loader){}\n",
                names.row_type_name(&table.name)?,
                comma
            )),
            CodeTableKind::Collection => content.raw_fmt(format_args!(
                "                {}.Load(loader){}\n",
                names.repository_type_name(&table.name)?,
                comma
            )),
        }
    }
    content.raw("            );\n        }\n    }\n\n");
    render_lazy_game_data_class(content, &model, names.access_modifier(), partial, &names)?;
    content.raw("}\n");
    Ok(())
}

fn validate_runtime_interface_options(options: &CSharpCodegenOptions) -> Result<()> {
    if matches!(
        options.runtime_interface_emit,
        RuntimeInterfaceEmit::Reference
    ) {
        return Err(validation_error(
            "runtime_interface_emit=reference is not supported for C# yet; use copy or none. C# JSON copy adapters target System.Text.Json; MessagePack copy adapters are legacy-bootstrap until runtime/csharp/messagepack_csharp_reader lands.",
        ));
    }
    if matches!(options.runtime_interface, RuntimeInterfaceMode::External) {
        if let Some(namespace) = &options.runtime_namespace {
            validate_csharp_namespace(namespace)?;
        }
    }
    validate_type_mappings(&options.type_mappings)?;
    for using in &options.using {
        validate_csharp_using(using)?;
    }
    Ok(())
}

fn validate_type_mappings(mappings: &CSharpTypeMappings) -> Result<()> {
    for (key, value) in [
        ("type_mappings.color", &mappings.color),
        ("type_mappings.vector2", &mappings.vector2),
        ("type_mappings.vector3", &mappings.vector3),
        ("type_mappings.vector4", &mappings.vector4),
        ("type_mappings.vector2_int", &mappings.vector2_int),
        ("type_mappings.vector3_int", &mappings.vector3_int),
        ("type_mappings.vector4_int", &mappings.vector4_int),
    ] {
        if let Some(value) = value {
            validate_csharp_type_reference(key, value)?;
        }
    }
    Ok(())
}

fn validate_csharp_type_reference(option: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.contains(';')
        || trimmed.contains('(')
        || trimmed.contains(')')
        || trimmed.chars().any(char::is_whitespace)
    {
        return Err(validation_error(format!(
            "invalid {option} `{value}`; expected a C# type reference"
        )));
    }
    Ok(())
}

fn validate_csharp_using(value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(';') || trimmed.starts_with("using ") {
        return Err(validation_error(format!(
            "invalid using `{value}`; expected a namespace without `using` or semicolon"
        )));
    }
    validate_csharp_namespace(trimmed)
}

fn csharp_table_data_type(table: &super::CodeTable, names: &CSharpNames) -> Result<String> {
    match table.kind {
        CodeTableKind::Object => names.row_type_name(&table.name),
        CodeTableKind::Collection => names.repository_type_name(&table.name),
    }
}

fn render_lazy_game_data_class(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    access_modifier: &str,
    partial: &str,
    names: &CSharpNames,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    {access_modifier}{partial} class {} : IGameDataAccess\n    {{\n        private readonly IConfigForgeRuntimeDataLoader _loader;\n",
        names.lazy_game_data_class_name()
    ));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "        private {}? _{};\n",
            csharp_table_data_type(table, names)?,
            names.local_name(&table.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "\n        public {}(IConfigForgeRuntimeDataLoader loader)\n        {{\n            _loader = loader;\n        }}\n",
        names.lazy_game_data_class_name()
    ));
    for table in &model.tables {
        let data_type = csharp_table_data_type(table, names)?;
        let property = names.property_name(&table.name)?;
        let cache = format!("_{}", names.local_name(&table.name)?);
        let load_expr = match table.kind {
            CodeTableKind::Object => format!("{}.Load(_loader)", names.row_type_name(&table.name)?),
            CodeTableKind::Collection => {
                format!("{}.Load(_loader)", names.repository_type_name(&table.name)?)
            }
        };
        content.raw_fmt(format_args!(
            "\n        public {data_type} {property}\n        {{\n            get\n            {{\n                if ({cache} is not null)\n                {{\n                    return {cache};\n                }}\n                var loaded = {load_expr};\n                {cache} = loaded;\n                return loaded;\n            }}\n        }}\n"
        ));
    }
    content.raw_fmt(format_args!(
        "\n        public {} PreloadAll()\n        {{\n            return {}.FromLoaded(\n",
        names.game_data_class_name(),
        names.game_data_class_name()
    ));
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        content.raw_fmt(format_args!(
            "                {}{}\n",
            names.property_name(&table.name)?,
            comma
        ));
    }
    content.raw("            );\n        }\n    }\n");
    Ok(())
}

fn render_runtime_contract(
    content: &mut impl CodeWriterLike,
    enums: &[super::CodeEnum],
    names: &CSharpNames,
) -> Result<()> {
    content.raw("    public static class ConfigForgeRuntimeContract\n    {\n");
    content.raw_fmt(format_args!(
        "        public const string ConfigForgeReaderContractVersion = {RUNTIME_READER_CONTRACT_VERSION:?};\n"
    ));
    content.raw_fmt(format_args!(
        "        public const string RequiredConfigForgeReaderContract = {REQUIRED_RUNTIME_READER_CONTRACT:?};\n"
    ));
    content.raw("    }\n\n");
    content.raw("    public interface IConfigForgeRuntimeDataLoader\n    {\n");
    content.raw("        IConfigForgeRuntimeDataReader Open(string path);\n");
    content.raw("    }\n\n");
    content.raw("    public interface IConfigForgeRuntimeDataReader\n    {\n");
    content.raw("        void EnterRequiredField(string name);\n");
    content.raw("        bool EnterOptionalField(string name);\n");
    content.raw("        void ExitField();\n");
    content.raw("        ulong? SchemaRevision();\n");
    content.raw("        int ReadRowCount();\n");
    content.raw("        void EnterRow(int index);\n");
    content.raw("        void ExitRow();\n");
    content.raw("        int ReadListLength();\n");
    content.raw("        void EnterListElement(int index);\n");
    content.raw("        void ExitListElement();\n");
    content.raw("        void BeginRecord(int optionalSlotCount);\n");
    content.raw("        void EndRecord();\n");
    content.raw("        int ReadEnumIndex(IReadOnlyList<string> variants);\n");
    content.raw("        void EnsureExhausted();\n");
    content.raw("        long ReadInt();\n");
    content.raw("        double ReadFloat();\n");
    content.raw("        bool ReadBool();\n");
    content.raw("        string ReadString();\n");
    content.raw("    }\n\n");
    content.raw("    public enum ConfigForgeRuntimeErrorKind\n    {\n        Io,\n        Decode,\n        SchemaMismatch,\n        TypeMismatch,\n        NotFound,\n        Unsupported,\n        InvalidPath,\n    }\n\n");
    content.raw("    public sealed class ConfigForgeRuntimeException : Exception\n    {\n");
    content.raw("        public ConfigForgeRuntimeException(string message) : this(ConfigForgeRuntimeErrorKind.Decode, message) { }\n");
    content.raw("        public ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind kind, string message) : base(message) { Kind = kind; }\n");
    content.raw("        public ConfigForgeRuntimeErrorKind Kind { get; }\n");
    content.raw("        public string Code => Kind switch\n        {\n            ConfigForgeRuntimeErrorKind.Io => \"CF_RT_IO\",\n            ConfigForgeRuntimeErrorKind.Decode => \"CF_RT_DECODE\",\n            ConfigForgeRuntimeErrorKind.SchemaMismatch => \"CF_RT_SCHEMA_MISMATCH\",\n            ConfigForgeRuntimeErrorKind.TypeMismatch => \"CF_RT_TYPE_MISMATCH\",\n            ConfigForgeRuntimeErrorKind.NotFound => \"CF_RT_NOT_FOUND\",\n            ConfigForgeRuntimeErrorKind.Unsupported => \"CF_RT_UNSUPPORTED\",\n            ConfigForgeRuntimeErrorKind.InvalidPath => \"CF_RT_INVALID_PATH\",\n            _ => \"CF_RT_INTERNAL\",\n        };\n");
    content.raw("        public string? Path { get; init; }\n");
    content.raw("    }\n\n");
    render_builtin_value_types(content);
    content.raw("    internal static class ConfigForgeRuntimeReader\n    {\n");
    content.raw("        public static long ReadIntValue(IConfigForgeRuntimeDataReader reader) => reader.ReadInt();\n");
    content.raw("        public static double ReadFloatValue(IConfigForgeRuntimeDataReader reader) => reader.ReadFloat();\n");
    content.raw("        public static bool ReadBoolValue(IConfigForgeRuntimeDataReader reader) => reader.ReadBool();\n");
    content.raw("        public static string ReadStringValue(IConfigForgeRuntimeDataReader reader) => reader.ReadString();\n\n");
    render_builtin_value_read_helpers(content);
    for code_enum in enums {
        let type_name = names.type_name(&code_enum.name)?;
        content.raw_fmt(format_args!(
            "        public static {type_name} Read{type_name}Value(IConfigForgeRuntimeDataReader reader)\n        {{\n            var index = reader.ReadEnumIndex(new[]\n            {{\n"
        ));
        for value in &code_enum.values {
            content.raw_fmt(format_args!("                {:?},\n", value.name));
        }
        content.raw("            });\n            return index switch\n            {\n");
        for (ordinal, value) in code_enum.values.iter().enumerate() {
            content.raw_fmt(format_args!(
                "                {ordinal} => {}.{},\n",
                type_name,
                names.enum_value_name(&value.name)?
            ));
        }
        content.raw_fmt(format_args!(
            "                _ => throw new ConfigForgeRuntimeException($\"Unknown {type_name} enum index '{{index}}'\"),\n            }};\n        }}\n\n"
        ));
    }
    content.raw("        public static T ReadRequiredField<T>(IConfigForgeRuntimeDataReader reader, string name, Func<IConfigForgeRuntimeDataReader, T> read)\n        {\n            reader.EnterRequiredField(name);\n            try\n            {\n                return read(reader);\n            }\n            finally\n            {\n                reader.ExitField();\n            }\n        }\n\n");
    content.raw("        public static T? ReadOptionalClassField<T>(IConfigForgeRuntimeDataReader reader, string name, Func<IConfigForgeRuntimeDataReader, T> read) where T : class\n        {\n            if (!reader.EnterOptionalField(name))\n            {\n                return null;\n            }\n            try\n            {\n                return read(reader);\n            }\n            finally\n            {\n                reader.ExitField();\n            }\n        }\n\n");
    content.raw("        public static T? ReadOptionalStructField<T>(IConfigForgeRuntimeDataReader reader, string name, Func<IConfigForgeRuntimeDataReader, T> read) where T : struct\n        {\n            if (!reader.EnterOptionalField(name))\n            {\n                return null;\n            }\n            try\n            {\n                return read(reader);\n            }\n            finally\n            {\n                reader.ExitField();\n            }\n        }\n\n");
    content.raw("        public static T[] ReadListValue<T>(IConfigForgeRuntimeDataReader reader, Func<IConfigForgeRuntimeDataReader, T> read)\n        {\n            var length = reader.ReadListLength();\n            var values = new T[length];\n            for (var index = 0; index < length; index++)\n            {\n                reader.EnterListElement(index);\n                try\n                {\n                    values[index] = read(reader);\n                }\n                finally\n                {\n                    reader.ExitListElement();\n                }\n            }\n            return values;\n        }\n");
    content.raw("\n        public static int LowerBound<TKey>((TKey Key, int RowIndex)[] entries, TKey key)\n        {\n            var comparer = Comparer<TKey>.Default;\n            var low = 0;\n            var high = entries.Length;\n            while (low < high)\n            {\n                var mid = low + ((high - low) / 2);\n                if (comparer.Compare(entries[mid].Key, key) < 0)\n                {\n                    low = mid + 1;\n                }\n                else\n                {\n                    high = mid;\n                }\n            }\n            return low;\n        }\n\n");
    content.raw("        public static int UpperBound<TKey>((TKey Key, int RowIndex)[] entries, TKey key)\n        {\n            var comparer = Comparer<TKey>.Default;\n            var low = 0;\n            var high = entries.Length;\n            while (low < high)\n            {\n                var mid = low + ((high - low) / 2);\n                if (comparer.Compare(entries[mid].Key, key) <= 0)\n                {\n                    low = mid + 1;\n                }\n                else\n                {\n                    high = mid;\n                }\n            }\n            return low;\n        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_builtin_value_types(content: &mut impl CodeWriterLike) {
    content.raw("    public readonly struct ConfigForgeColor\n    {\n        public ConfigForgeColor(double r, double g, double b, double a)\n        {\n            R = r;\n            G = g;\n            B = b;\n            A = a;\n        }\n\n        public double R { get; }\n        public double G { get; }\n        public double B { get; }\n        public double A { get; }\n    }\n\n");
    for dimension in 2..=4 {
        render_vector_value_type(content, dimension, false);
        render_vector_value_type(content, dimension, true);
    }
}

fn render_vector_value_type(content: &mut impl CodeWriterLike, dimension: usize, int_vector: bool) {
    let suffix = if int_vector { "Int" } else { "" };
    let value_type = if int_vector { "long" } else { "double" };
    let type_name = format!("ConfigForgeVector{dimension}{suffix}");
    let parameters = (0..dimension)
        .map(|index| format!("{value_type} v{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    content.raw_fmt(format_args!(
        "    public readonly struct {type_name}\n    {{\n        public {type_name}({parameters})\n        {{\n"
    ));
    for index in 0..dimension {
        let property = ["X", "Y", "Z", "W"][index];
        content.raw_fmt(format_args!("            {property} = v{};\n", index + 1));
    }
    content.raw("        }\n\n");
    for index in 0..dimension {
        let property = ["X", "Y", "Z", "W"][index];
        content.raw_fmt(format_args!(
            "        public {value_type} {property} {{ get; }}\n"
        ));
    }
    content.raw("    }\n\n");
}

fn render_builtin_value_read_helpers(content: &mut impl CodeWriterLike) {
    content.raw("        public static ConfigForgeColor ReadColorValue(IConfigForgeRuntimeDataReader reader)\n        {\n            return ReadColorValue(reader, (r, g, b, a) => new ConfigForgeColor(r, g, b, a));\n        }\n\n        public static T ReadColorValue<T>(IConfigForgeRuntimeDataReader reader, Func<double, double, double, double, T> create)\n        {\n            reader.BeginRecord(0);\n            try\n            {\n                var r = ReadRequiredField(reader, \"r\", ReadFloatValue);\n                var g = ReadRequiredField(reader, \"g\", ReadFloatValue);\n                var b = ReadRequiredField(reader, \"b\", ReadFloatValue);\n                var a = ReadRequiredField(reader, \"a\", ReadFloatValue);\n                return create(r, g, b, a);\n            }\n            finally\n            {\n                reader.EndRecord();\n            }\n        }\n\n");
    for dimension in 2..=4 {
        render_vector_read_helper(content, dimension, false);
        render_vector_read_helper(content, dimension, true);
    }
}

fn render_vector_read_helper(
    content: &mut impl CodeWriterLike,
    dimension: usize,
    int_vector: bool,
) {
    let suffix = if int_vector { "Int" } else { "" };
    let type_name = format!("ConfigForgeVector{dimension}{suffix}");
    let helper_name = format!("Read{type_name}Value");
    let fallback_args = (0..dimension)
        .map(|index| {
            if int_vector {
                format!("(long)v{}", index + 1)
            } else {
                format!("v{}", index + 1)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let generic_type = std::iter::repeat_n("double", dimension)
        .chain(std::iter::once("T"))
        .collect::<Vec<_>>()
        .join(", ");
    let component_reads = (0..dimension)
        .map(|index| {
            format!(
                "                var v{} = ReadRequiredField(reader, components[{}], ReadFloatValue);\n",
                index + 1,
                index
            )
        })
        .collect::<String>();
    let create_args = (0..dimension)
        .map(|index| format!("v{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    content.raw_fmt(format_args!(
        "        public static {type_name} {helper_name}(IConfigForgeRuntimeDataReader reader, IReadOnlyList<string> components)\n        {{\n            return {helper_name}(reader, components, ({create_args}) => new {type_name}({fallback_args}));\n        }}\n\n        public static T {helper_name}<T>(IConfigForgeRuntimeDataReader reader, IReadOnlyList<string> components, Func<{generic_type}> create)\n        {{\n            if (components.Count != {dimension})\n            {{\n                throw new ConfigForgeRuntimeException(\"Vector component count mismatch.\");\n            }}\n            reader.BeginRecord(0);\n            try\n            {{\n{component_reads}                return create({create_args});\n            }}\n            finally\n            {{\n                reader.EndRecord();\n            }}\n        }}\n\n"
    ));
}

fn render_standard_runtime_adapters(content: &mut impl CodeWriterLike) {
    render_csharp_standard_runtime_adapter(
        content,
        "Json",
        "ConfigForge.Runtime.SystemTextJsonReader.SystemTextJsonRuntimeDataFileLoader",
        "ConfigForge.Runtime.SystemTextJsonReader.SystemTextJsonRuntimeDataReader",
        "System.Text.Json",
        "stable",
    );
    render_csharp_standard_runtime_adapter(
        content,
        "MessagePack",
        "ConfigForge.Runtime.MessagePackReader.MessagePackRuntimeDataFileLoader",
        "ConfigForge.Runtime.MessagePackReader.MessagePackRuntimeDataReader",
        "configforge-dependency-free-messagepack-subset",
        "legacy-bootstrap",
    );
    render_csharp_standard_runtime_adapter(
        content,
        "Binary",
        "ConfigForge.Runtime.BinaryReader.BinaryRuntimeDataFileLoader",
        "ConfigForge.Runtime.BinaryReader.BinaryRuntimeDataReader",
        "configforge-binary",
        "stable",
    );
}

fn render_csharp_standard_runtime_adapter(
    content: &mut impl CodeWriterLike,
    format_name: &str,
    loader_type: &str,
    reader_type: &str,
    parser_backend: &str,
    support_level: &str,
) {
    content.raw_fmt(format_args!(
        "    // {format_name} runtime adapter backend: {parser_backend}; support level: {support_level}.\n"
    ));
    content.raw_fmt(format_args!(
        "    public sealed class ConfigForge{format_name}RuntimeDataLoaderAdapter : IConfigForgeRuntimeDataLoader\n    {{\n"
    ));
    content.raw_fmt(format_args!(
        "        private readonly {loader_type} _loader;\n\n"
    ));
    content.raw_fmt(format_args!(
        "        public ConfigForge{format_name}RuntimeDataLoaderAdapter(string root)\n            : this(new {loader_type}(root))\n        {{\n        }}\n\n"
    ));
    content.raw_fmt(format_args!(
        "        public ConfigForge{format_name}RuntimeDataLoaderAdapter({loader_type} loader)\n        {{\n            _loader = loader ?? throw new ArgumentNullException(nameof(loader));\n        }}\n\n"
    ));
    content.raw("        public int OpenCount => _loader.OpenCount;\n\n");
    content.raw_fmt(format_args!(
        "        public IConfigForgeRuntimeDataReader Open(string path) => new ConfigForge{format_name}RuntimeDataReaderAdapter(_loader.OpenReader(path));\n"
    ));
    content.raw("    }\n\n");

    content.raw_fmt(format_args!(
        "    public sealed class ConfigForge{format_name}RuntimeDataReaderAdapter : IConfigForgeRuntimeDataReader\n    {{\n"
    ));
    content.raw_fmt(format_args!(
        "        private readonly {reader_type} _reader;\n\n"
    ));
    content.raw_fmt(format_args!(
        "        public ConfigForge{format_name}RuntimeDataReaderAdapter({reader_type} reader)\n        {{\n            _reader = reader ?? throw new ArgumentNullException(nameof(reader));\n        }}\n\n"
    ));
    content.raw("        public void EnterRequiredField(string name) => _reader.EnterRequiredField(name);\n");
    content.raw("        public bool EnterOptionalField(string name) => _reader.EnterOptionalField(name);\n");
    content.raw("        public void ExitField() => _reader.ExitField();\n");
    content.raw("        public ulong? SchemaRevision() => _reader.SchemaRevision();\n");
    content.raw("        public int ReadRowCount() => _reader.ReadRowCount();\n");
    content.raw("        public void EnterRow(int index) => _reader.EnterRow(index);\n");
    content.raw("        public void ExitRow() => _reader.ExitRow();\n");
    content.raw("        public int ReadListLength() => _reader.ReadListLength();\n");
    content.raw(
        "        public void EnterListElement(int index) => _reader.EnterListElement(index);\n",
    );
    content.raw("        public void ExitListElement() => _reader.ExitListElement();\n");
    content.raw("        public void BeginRecord(int optionalSlotCount) => _reader.BeginRecord(optionalSlotCount);\n");
    content.raw("        public void EndRecord() => _reader.EndRecord();\n");
    content.raw("        public int ReadEnumIndex(IReadOnlyList<string> variants) => _reader.ReadEnumIndex(variants);\n");
    content.raw("        public void EnsureExhausted() => _reader.EnsureExhausted();\n");
    content.raw("        public long ReadInt() => _reader.ReadInt();\n");
    content.raw("        public double ReadFloat() => _reader.ReadFloat();\n");
    content.raw("        public bool ReadBool() => _reader.ReadBool();\n");
    content.raw("        public string ReadString() => _reader.ReadString();\n");
    content.raw("    }\n\n");
}

fn render_game_data_access_interface(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    access_modifier: &str,
    names: &CSharpNames,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    {access_modifier} interface IGameDataAccess\n    {{\n"
    ));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "        {} {} {{ get; }}\n",
            match table.kind {
                CodeTableKind::Object => names.row_type_name(&table.name)?,
                CodeTableKind::Collection => names.repository_type_name(&table.name)?,
            },
            names.property_name(&table.name)?
        ));
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_csharp_i18n_using(content: &mut impl CodeWriterLike, i18n: &I18nOptions) {
    if !matches!(i18n.mode, I18nMode::GlobalFunction) {
        return;
    }
    if let Some(using) = i18n.csharp_using.as_deref() {
        let trimmed = using.trim();
        if trimmed.starts_with("using ") {
            content.raw(trimmed);
            if !trimmed.ends_with(';') {
                content.raw(";");
            }
            content.newline();
        } else {
            content.raw_fmt(format_args!("using {trimmed};\n"));
        }
    }
}

fn render_custom_csharp_usings(content: &mut impl CodeWriterLike, usings: &[String]) {
    for using in usings {
        content.raw_fmt(format_args!("using {};\n", using.trim()));
    }
}

fn render_csharp_i18n_prelude(content: &mut impl CodeWriterLike, i18n: &I18nOptions) {
    if matches!(i18n.mode, I18nMode::Generic) {
        content.raw(
            "    public interface ITextTranslator\n    {\n        string? Translate(string context, string text);\n    }\n\n",
        );
    }
}

fn render_csharp_class(
    content: &mut impl CodeWriterLike,
    class_modifier: &str,
    type_name: &str,
    fields: &[CodeField],
    names: &CSharpNames,
    i18n: &I18nOptions,
    text_context: &str,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    {class_modifier} class {type_name}\n    {{\n"
    ));
    for field in fields {
        content.raw_fmt(format_args!(
            "        public {} {} {{ get; init; }}{}\n",
            names.csharp_field_type(field)?,
            names.property_name(&field.name)?,
            names.field_initializer(field)?
        ));
    }
    render_csharp_text_helpers(content, text_context, fields, names, i18n)?;
    content.raw("    }\n\n");
    Ok(())
}

fn render_read_method(
    content: &mut impl CodeWriterLike,
    type_name: &str,
    fields: &[CodeField],
    names: &CSharpNames,
) -> Result<()> {
    let reader_shape = ReaderShapePlan::for_fields(fields);
    content.raw_fmt(format_args!(
        "    internal static partial class {type_name}Reader\n    {{\n        public static {type_name} Read(IConfigForgeRuntimeDataReader reader)\n        {{\n            reader.BeginRecord({});\n            try\n            {{\n                return new {type_name}\n                {{\n",
        reader_shape.optional_slot_count
    ));
    for field in fields {
        content.raw_fmt(format_args!(
            "                    {} = {},\n",
            names.property_name(&field.name)?,
            names.read_field_expression(field)?
        ));
    }
    content.raw("                };\n            }\n            finally\n            {\n                reader.EndRecord();\n            }\n        }\n    }\n\n");
    Ok(())
}

fn render_csharp_row_class(
    content: &mut impl CodeWriterLike,
    access_modifier: &str,
    partial: &str,
    type_name: &str,
    table: &super::CodeTable,
    names: &CSharpNames,
    i18n: &I18nOptions,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    {access_modifier}{partial} class {type_name}\n    {{\n"
    ));
    for field in &table.fields {
        content.raw_fmt(format_args!(
            "        public {} {} {{ get; init; }}{}\n",
            names.csharp_field_type(field)?,
            names.property_name(&field.name)?,
            names.field_initializer(field)?
        ));
    }
    for fragment in &table.fragments {
        content.raw_fmt(format_args!(
            "        public {} {} {{ get; init; }} = new();\n",
            names.fragment_payload_type_name(table, fragment)?,
            names.property_name(&fragment.name)?
        ));
    }
    render_csharp_text_helpers(content, &table.name, &table.fields, names, i18n)?;
    if table.kind == CodeTableKind::Object {
        content.raw_fmt(format_args!(
            "\n        internal const string DataFile = {:?};\n",
            table.data_file
        ));
        if let Some(revision) = table.schema_revision {
            content.raw_fmt(format_args!(
                "        internal const ulong ExpectedSchemaRevision = {revision}UL;\n"
            ));
        }
        content.raw_fmt(format_args!(
            "\n        internal static {type_name} Load(IConfigForgeRuntimeDataLoader loader)\n        {{\n            var reader = loader.Open(DataFile);\n"
        ));
        if table.schema_revision.is_some() {
            content.raw(
                "            var schemaRevision = reader.SchemaRevision();\n            if (schemaRevision.HasValue && schemaRevision.Value != ExpectedSchemaRevision)\n            {\n                throw new ConfigForgeRuntimeException($\"Schema revision mismatch for '{DataFile}': expected {ExpectedSchemaRevision}, got {schemaRevision.Value}\");\n            }\n",
            );
        }
        content.raw_fmt(format_args!(
            "            var value = {type_name}Reader.Read(reader);\n            reader.EnsureExhausted();\n            return value;\n        }}\n"
        ));
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_csharp_text_helpers(
    content: &mut impl CodeWriterLike,
    context_prefix: &str,
    fields: &[CodeField],
    names: &CSharpNames,
    i18n: &I18nOptions,
) -> Result<()> {
    for field in fields
        .iter()
        .filter(|field| matches!(field.code_type, super::CodeTypeRef::Text))
    {
        let function_name = names.text_function_name(&field.name)?;
        let field_name = names.property_name(&field.name)?;
        let context = format!("{context_prefix}.{}", field.name);
        match i18n.mode {
            I18nMode::Generic if field.required => {
                content.raw_fmt(format_args!(
                    "\n        public string {function_name}(ITextTranslator translator)\n        {{\n            var translated = translator.Translate({context:?}, {field_name});\n            return translated ?? {field_name};\n        }}\n"
                ));
            }
            I18nMode::Generic => {
                content.raw_fmt(format_args!(
                    "\n        public string? {function_name}(ITextTranslator translator)\n        {{\n            if ({field_name} is null)\n            {{\n                return null;\n            }}\n            var translated = translator.Translate({context:?}, {field_name});\n            return translated ?? {field_name};\n        }}\n"
                ));
            }
            I18nMode::GlobalFunction if field.required => {
                content.raw_fmt(format_args!(
                    "\n        public string {function_name}()\n        {{\n            {statement}\n        }}\n",
                    statement = csharp_global_i18n_statement(i18n, &context, &field_name, &field_name)
                ));
            }
            I18nMode::GlobalFunction => {
                content.raw_fmt(format_args!(
                    "\n        public string? {function_name}()\n        {{\n            if ({field_name} is null)\n            {{\n                return null;\n            }}\n            {statement}\n        }}\n",
                    statement = csharp_global_i18n_statement(i18n, &context, &field_name, &field_name)
                ));
            }
        }
    }
    Ok(())
}

fn csharp_global_i18n_statement(
    i18n: &I18nOptions,
    context: &str,
    msgid_expr: &str,
    fallback_expr: &str,
) -> String {
    let function = i18n.function.as_deref().expect("global i18n function");
    match i18n.returns {
        I18nFunctionReturn::String => format!("return {function}({context:?}, {msgid_expr});"),
        I18nFunctionReturn::Optional => format!(
            "var translated = {function}({context:?}, {msgid_expr});\n            return translated ?? {fallback_expr};"
        ),
    }
}

fn render_row_relation_methods(
    content: &mut impl CodeWriterLike,
    table: &super::CodeTable,
    model: &CodeModel,
    names: &CSharpNames,
) -> Result<()> {
    let linked = table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
        .collect::<Vec<_>>();
    if linked.is_empty() {
        return Ok(());
    }
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "    public partial class {row_type}\n    {{\n"
    ));
    for foreign_key in &linked {
        let cache = names.relation_cache_field_name(foreign_key)?;
        match foreign_key
            .linked_field
            .as_ref()
            .map(|linked| linked.cardinality)
            .unwrap_or(CodeRelationCardinality::One)
        {
            CodeRelationCardinality::One | CodeRelationCardinality::OptionalOne => {
                content.raw_fmt(format_args!("        private int {cache} = -1;\n"));
            }
            CodeRelationCardinality::Many => {
                content.raw_fmt(format_args!("        private int[]? {cache};\n"));
            }
        }
    }
    content.raw("\n");
    for foreign_key in linked {
        let target_table = model
            .tables
            .iter()
            .find(|table| table.name == foreign_key.target_table)
            .ok_or_else(|| {
                validation_error(format!(
                    "missing target table `{}`",
                    foreign_key.target_table
                ))
            })?;
        let target_index = target_table
            .indexes
            .iter()
            .find(|index| index.name == foreign_key.target_index)
            .ok_or_else(|| {
                validation_error(format!(
                    "missing target index `{}`",
                    foreign_key.target_index
                ))
            })?;
        let method = names.relation_function_name(foreign_key)?;
        let target_row = names.row_type_name(&foreign_key.target_table)?;
        let target_property = names.property_name(&foreign_key.target_table)?;
        let target_lookup = names.index_function_name(target_index)?;
        let args = foreign_key
            .fields
            .iter()
            .map(|field| {
                let prop = names.property_name(&field.name)?;
                if !field.required && names.is_value_type(&field.code_type) {
                    Ok(format!("{prop}.Value"))
                } else {
                    Ok(prop)
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        match foreign_key
            .linked_field
            .as_ref()
            .map(|linked| linked.cardinality)
            .unwrap_or(CodeRelationCardinality::One)
        {
            CodeRelationCardinality::One => {
                content.raw_fmt(format_args!(
                    "        public {target_row} {method}(IGameDataAccess data)\n        {{\n            return data.{target_property}.{target_lookup}({args}) ?? throw new ConfigForgeRuntimeException(\"Missing relation target for {row_type}.{method}\");\n        }}\n\n"
                ));
            }
            CodeRelationCardinality::OptionalOne => {
                let guard = foreign_key
                    .fields
                    .iter()
                    .filter(|field| !field.required)
                    .map(|field| {
                        let prop = names.property_name(&field.name)?;
                        if names.is_value_type(&field.code_type) {
                            Ok(format!("!{prop}.HasValue"))
                        } else {
                            Ok(format!("{prop} is null"))
                        }
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(" || ");
                content.raw_fmt(format_args!(
                    "        public {target_row}? {method}(IGameDataAccess data)\n        {{\n"
                ));
                if !guard.is_empty() {
                    content.raw_fmt(format_args!(
                        "            if ({guard})\n            {{\n                return null;\n            }}\n"
                    ));
                }
                content.raw_fmt(format_args!(
                    "            return data.{target_property}.{target_lookup}({args});\n        }}\n\n"
                ));
            }
            CodeRelationCardinality::Many => {
                let field = &foreign_key.fields[0];
                let prop = names.property_name(&field.name)?;
                content.raw_fmt(format_args!(
                    "        public IReadOnlyList<{target_row}> {method}(IGameDataAccess data)\n        {{\n            var rows = new List<{target_row}>();\n            foreach (var key in {prop})\n            {{\n                var row = data.{target_property}.{target_lookup}(key);\n                if (row != null)\n                {{\n                    rows.Add(row);\n                }}\n            }}\n            return rows;\n        }}\n\n"
                ));
            }
        }
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_row_read_method(
    content: &mut impl CodeWriterLike,
    type_name: &str,
    table: &super::CodeTable,
    names: &CSharpNames,
) -> Result<()> {
    let reader_shape = ReaderShapePlan::for_table(table);
    content.raw_fmt(format_args!(
        "    internal static partial class {type_name}Reader\n    {{\n        public static {type_name} Read(IConfigForgeRuntimeDataReader reader)\n        {{\n            reader.BeginRecord({});\n            try\n            {{\n                return new {type_name}\n                {{\n",
        reader_shape.optional_slot_count
    ));
    for field in &table.fields {
        content.raw_fmt(format_args!(
            "                    {} = {},\n",
            names.property_name(&field.name)?,
            names.read_field_expression(field)?
        ));
    }
    for fragment in reader_shape.fragments {
        let payload_type = names.fragment_payload_type_name(table, fragment)?;
        content.raw_fmt(format_args!(
            "                    {} = ConfigForgeRuntimeReader.ReadOptionalClassField(reader, {:?}, {}Reader.Read) ?? new {}(),\n",
            names.property_name(&fragment.name)?,
            fragment.name,
            payload_type,
            payload_type
        ));
    }
    content.raw("                };\n            }\n            finally\n            {\n                reader.EndRecord();\n            }\n        }\n    }\n\n");
    Ok(())
}

fn render_table_class(
    content: &mut impl CodeWriterLike,
    access_modifier: &str,
    partial: &str,
    table: &super::CodeTable,
    names: &CSharpNames,
) -> Result<()> {
    let table_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "    {access_modifier}{partial} class {table_type}\n    {{\n"
    ));
    content.raw_fmt(format_args!(
        "        public const string DataFile = {:?};\n",
        table.data_file
    ));
    if let Some(revision) = table.schema_revision {
        content.raw_fmt(format_args!(
            "        public const ulong ExpectedSchemaRevision = {revision}UL;\n"
        ));
    }
    content.raw_fmt(format_args!(
        "        private readonly {}[] _rows;\n",
        row_type
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            content.raw_fmt(format_args!(
                "        private readonly ({key_type} Key, int RowIndex)[] {storage};\n"
            ));
        } else if index.unique {
            content.raw_fmt(format_args!(
                "        private readonly Dictionary<{key_type}, int> {storage};\n"
            ));
        } else {
            content.raw_fmt(format_args!(
                "        private readonly Dictionary<{key_type}, int[]> {storage};\n"
            ));
        }
    }
    content.raw("\n");
    content.raw_fmt(format_args!(
        "        private {table_type}({row_type}[] rows"
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            content.raw_fmt(format_args!(", ({key_type} Key, int RowIndex)[] {storage}"));
        } else if index.unique {
            content.raw_fmt(format_args!(", Dictionary<{key_type}, int> {storage}"));
        } else {
            content.raw_fmt(format_args!(", Dictionary<{key_type}, int[]> {storage}"));
        }
    }
    content.raw(")\n        {\n            _rows = rows;\n");
    for index in &table.indexes {
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!("            this.{storage} = {storage};\n"));
    }
    content.raw("        }\n\n");
    content.raw_fmt(format_args!(
        "        internal static {table_type} Load(IConfigForgeRuntimeDataLoader loader)\n        {{\n            var reader = loader.Open(DataFile);\n"
    ));
    if table.schema_revision.is_some() {
        content.raw(
            "            var schemaRevision = reader.SchemaRevision();\n            if (schemaRevision.HasValue && schemaRevision.Value != ExpectedSchemaRevision)\n            {\n                throw new ConfigForgeRuntimeException($\"Schema revision mismatch for '{DataFile}': expected {ExpectedSchemaRevision}, got {schemaRevision.Value}\");\n            }\n",
        );
    }
    content.raw_fmt(format_args!(
        "            var rowCount = reader.ReadRowCount();\n            var rows = new {row_type}[rowCount];\n            for (var rowIndex = 0; rowIndex < rowCount; rowIndex++)\n            {{\n                reader.EnterRow(rowIndex);\n                try\n                {{\n                    rows[rowIndex] = {row_type}Reader.Read(reader);\n                }}\n                finally\n                {{\n                    reader.ExitRow();\n                }}\n            }}\n            reader.EnsureExhausted();\n            return FromRows(rows);\n        }}\n"
    ));
    content.raw_fmt(format_args!(
        "\n        public int Count => _rows.Length;\n\n        public {row_type}? RowAt(int rowIndex)\n        {{\n            return rowIndex >= 0 && rowIndex < _rows.Length ? _rows[rowIndex] : null;\n        }}\n"
    ));
    render_table_from_rows_method(content, table, names)?;
    render_index_lookup_methods(content, table, names)?;
    if table
        .foreign_keys
        .iter()
        .any(|foreign_key| foreign_key.linked_field.is_some())
    {
        content.raw("\n        public void Link(IGameDataAccess data)\n        {\n");
        for foreign_key in table
            .foreign_keys
            .iter()
            .filter(|foreign_key| foreign_key.linked_field.is_some())
        {
            let method = names.relation_function_name(foreign_key)?;
            content.raw_fmt(format_args!(
                "            foreach (var row in _rows)\n            {{\n                _ = row.{method}(data);\n            }}\n"
            ));
        }
        content.raw("        }\n");
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_compound_key_type(
    content: &mut impl CodeWriterLike,
    table_name: &str,
    index: &super::CodeIndex,
    access_modifier: &str,
    names: &CSharpNames,
) -> Result<()> {
    let key_type = names.compound_key_type_name(table_name, index)?;
    content.raw_fmt(format_args!(
        "    {access_modifier} readonly struct {key_type} : IEquatable<{key_type}>, IComparable<{key_type}>\n    {{\n"
    ));
    for field in &index.fields {
        content.raw_fmt(format_args!(
            "        public {} {} {{ get; }}\n",
            names.csharp_type(&field.code_type)?,
            names.property_name(&field.name)?
        ));
    }
    content.raw_fmt(format_args!("\n        public {key_type}("));
    content.raw_fmt(format_args!("{}", names.index_parameters(index)?));
    content.raw(")\n        {\n");
    for field in &index.fields {
        content.raw_fmt(format_args!(
            "            {} = {};\n",
            names.property_name(&field.name)?,
            names.local_name(&field.name)?
        ));
    }
    content.raw("        }\n\n");
    content.raw_fmt(format_args!(
        "        public bool Equals({key_type} other)\n        {{\n            return "
    ));
    content.raw_fmt(format_args!(
        "{}",
        index
            .fields
            .iter()
            .map(|field| {
                Ok(format!(
                    "EqualityComparer<{}>.Default.Equals({}, other.{})",
                    names.csharp_type(&field.code_type)?,
                    names.property_name(&field.name)?,
                    names.property_name(&field.name)?
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join(" && ")
    ));
    content.raw(";\n        }\n\n");
    content.raw_fmt(format_args!(
        "        public int CompareTo({key_type} other)\n        {{\n"
    ));
    for field in &index.fields {
        content.raw_fmt(format_args!(
            "            var {}Comparison = Comparer<{}>.Default.Compare({}, other.{});\n            if ({}Comparison != 0)\n            {{\n                return {}Comparison;\n            }}\n",
            names.local_name(&field.name)?,
            names.csharp_type(&field.code_type)?,
            names.property_name(&field.name)?,
            names.property_name(&field.name)?,
            names.local_name(&field.name)?,
            names.local_name(&field.name)?
        ));
    }
    content.raw("            return 0;\n        }\n\n");
    content.raw_fmt(format_args!(
        "        public override bool Equals(object? obj) => obj is {key_type} other && Equals(other);\n"
    ));
    content.raw("        public override int GetHashCode() => HashCode.Combine(");
    content.raw_fmt(format_args!(
        "{}",
        index
            .fields
            .iter()
            .map(|field| names.property_name(&field.name))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ));
    content.raw(");\n    }\n\n");
    Ok(())
}

fn render_table_from_rows_method(
    content: &mut impl CodeWriterLike,
    table: &super::CodeTable,
    names: &CSharpNames,
) -> Result<()> {
    let table_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "\n        private static {table_type} FromRows({row_type}[] rows)\n        {{\n"
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            content.raw_fmt(format_args!(
                "            var {storage} = new ({key_type} Key, int RowIndex)[rows.Length];\n"
            ));
        } else if index.unique {
            content.raw_fmt(format_args!(
                "            var {storage} = new Dictionary<{key_type}, int>();\n"
            ));
        } else {
            content.raw_fmt(format_args!(
                "            var {storage}Builder = new Dictionary<{key_type}, List<int>>();\n"
            ));
        }
    }
    if !table.indexes.is_empty() {
        content.raw("            for (var rowIndex = 0; rowIndex < rows.Length; rowIndex++)\n            {\n                var row = rows[rowIndex];\n");
        for index in &table.indexes {
            let key_expr = names.index_key_from_row(&table.name, index, "row")?;
            let storage = names.index_storage_name(index)?;
            if index.ordered {
                content.raw_fmt(format_args!(
                    "                {storage}[rowIndex] = ({key_expr}, rowIndex);\n"
                ));
            } else if index.unique {
                content.raw_fmt(format_args!(
                    "                var {storage}Key = {key_expr};\n                if ({storage}.ContainsKey({storage}Key))\n                {{\n                    throw new ConfigForgeRuntimeException(\"Duplicate unique index key for {table_type}.{index_name}\");\n                }}\n                {storage}.Add({storage}Key, rowIndex);\n",
                    index_name = index.name
                ));
            } else {
                content.raw_fmt(format_args!(
                    "                var {storage}Key = {key_expr};\n                if (!{storage}Builder.TryGetValue({storage}Key, out var {storage}Rows))\n                {{\n                    {storage}Rows = new List<int>();\n                    {storage}Builder.Add({storage}Key, {storage}Rows);\n                }}\n                {storage}Rows.Add(rowIndex);\n"
                ));
            }
        }
        content.raw("            }\n");
    }
    for index in table.indexes.iter().filter(|index| index.ordered) {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!(
            "            Array.Sort({storage}, (left, right) => Comparer<{key_type}>.Default.Compare(left.Key, right.Key));\n"
        ));
        if index.unique {
            content.raw_fmt(format_args!(
                "            for (var index = 1; index < {storage}.Length; index++)\n            {{\n                if (Comparer<{key_type}>.Default.Compare({storage}[index - 1].Key, {storage}[index].Key) == 0)\n                {{\n                    throw new ConfigForgeRuntimeException(\"Duplicate unique index key for {table_type}.{index_name}\");\n                }}\n            }}\n",
                index_name = index.name
            ));
        }
    }
    for index in table
        .indexes
        .iter()
        .filter(|index| !index.ordered && !index.unique)
    {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!(
            "            var {storage} = new Dictionary<{key_type}, int[]>();\n            foreach (var entry in {storage}Builder)\n            {{\n                {storage}.Add(entry.Key, entry.Value.ToArray());\n            }}\n"
        ));
    }
    content.raw_fmt(format_args!("            return new {table_type}(rows"));
    for index in &table.indexes {
        content.raw_fmt(format_args!(", {}", names.index_storage_name(index)?));
    }
    content.raw(");\n        }\n");
    Ok(())
}

fn render_index_lookup_methods(
    content: &mut impl CodeWriterLike,
    table: &super::CodeTable,
    names: &CSharpNames,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    for index in &table.indexes {
        let method = names.index_function_name(index)?;
        let parameters = names.index_parameters(index)?;
        let key = names.index_key_from_parameters(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            let lower_method = names.ordered_lower_bound_function_name(index)?;
            let upper_method = names.ordered_upper_bound_function_name(index)?;
            let range_method = names.ordered_range_function_name(index)?;
            if index.unique {
                content.raw_fmt(format_args!(
                    "\n        public {row_type}? {method}({parameters})\n        {{\n            var key = {key};\n            var index = ConfigForgeRuntimeReader.LowerBound({storage}, key);\n            if (index < {storage}.Length && Comparer<{}>.Default.Compare({storage}[index].Key, key) == 0)\n            {{\n                return _rows[{storage}[index].RowIndex];\n            }}\n            return null;\n        }}\n",
                    names.index_key_type(&table.name, index)?
                ));
            } else {
                content.raw_fmt(format_args!(
                    "\n        public IReadOnlyList<{row_type}> {method}({parameters})\n        {{\n            var key = {key};\n            var start = ConfigForgeRuntimeReader.LowerBound({storage}, key);\n            var end = ConfigForgeRuntimeReader.UpperBound({storage}, key);\n            return RowsFromOrderedRange({storage}, start, end);\n        }}\n"
                ));
            }
            content.raw_fmt(format_args!(
                "\n        public int {lower_method}({parameters})\n        {{\n            return ConfigForgeRuntimeReader.LowerBound({storage}, {key});\n        }}\n\n        public int {upper_method}({parameters})\n        {{\n            return ConfigForgeRuntimeReader.UpperBound({storage}, {key});\n        }}\n\n        public IReadOnlyList<{row_type}> {range_method}({min_parameters}, {max_parameters})\n        {{\n            var start = ConfigForgeRuntimeReader.LowerBound({storage}, {key_min});\n            var end = ConfigForgeRuntimeReader.UpperBound({storage}, {key_max});\n            return RowsFromOrderedRange({storage}, start, end);\n        }}\n",
                min_parameters = names.index_parameters_with_suffix(index, "Min")?,
                max_parameters = names.index_parameters_with_suffix(index, "Max")?,
                key_min = names.index_key_from_parameter_suffix(&table.name, index, "Min")?,
                key_max = names.index_key_from_parameter_suffix(&table.name, index, "Max")?
            ));
        } else if index.unique {
            content.raw_fmt(format_args!(
                "\n        public {row_type}? {method}({parameters})\n        {{\n            return {storage}.TryGetValue({key}, out var rowIndex) ? _rows[rowIndex] : null;\n        }}\n"
            ));
        } else {
            content.raw_fmt(format_args!(
                "\n        public IReadOnlyList<{row_type}> {method}({parameters})\n        {{\n            if (!{storage}.TryGetValue({key}, out var rowIndexes))\n            {{\n                return Array.Empty<{row_type}>();\n            }}\n            var rows = new {row_type}[rowIndexes.Length];\n            for (var index = 0; index < rowIndexes.Length; index++)\n            {{\n                rows[index] = _rows[rowIndexes[index]];\n            }}\n            return rows;\n        }}\n"
            ));
        }
    }
    if table.indexes.iter().any(|index| index.ordered) {
        content.raw_fmt(format_args!(
            "\n        private {row_type}[] RowsFromOrderedRange<TKey>((TKey Key, int RowIndex)[] entries, int start, int end)\n        {{\n            var length = Math.Max(0, end - start);\n            var rows = new {row_type}[length];\n            for (var offset = 0; offset < length; offset++)\n            {{\n                rows[offset] = _rows[entries[start + offset].RowIndex];\n            }}\n            return rows;\n        }}\n"
        ));
    }
    Ok(())
}

fn validation_error(message: impl Into<String>) -> ConfigForgeError {
    let message = format!("csharp codegen: {}", message.into());
    crate::code_artifact::generator_option_error(
        CF_GENERATOR_INVALID_OPTION,
        "generator.csharp.options",
        message.clone(),
        message,
        "valid C# codegen option value",
        "Check the csharp export task options and generated identifier settings.",
    )
}
