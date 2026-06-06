// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::{ConfigForgeError, Result, artifact::ArtifactSink};

use super::{
    CaseStyle, CheckedStreamingCodeWriter, CodeEnum, CodeExportInput, CodeField,
    CodeFragmentPayload, CodeGenerator, CodeIndex, CodeModel, CodeTable, CodeTableKind,
    CodeTypeRef, CodeWriterConfig, CodeWriterLike, NamingConfig, build_code_model,
    ir::{CodeRelationCardinality, require_non_empty_index_fields},
};

mod symbols;
mod types;

use symbols::validate_generated_symbols;
use types::JavaNames;

#[cfg(test)]
mod tests;

pub const LANGUAGE: &str = "java";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct JavaCodegenOptions {
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub root_class_name: Option<String>,
    #[serde(default)]
    pub game_data_class_name: Option<String>,
    #[serde(default)]
    pub lazy_class_name: Option<String>,
    #[serde(default = "default_access_modifier")]
    pub access_modifier: String,
    #[serde(default = "default_emit_final")]
    pub emit_final: bool,
}

impl Default for JavaCodegenOptions {
    fn default() -> Self {
        Self {
            package: None,
            root_class_name: None,
            game_data_class_name: None,
            lazy_class_name: None,
            access_modifier: default_access_modifier(),
            emit_final: default_emit_final(),
        }
    }
}

fn default_access_modifier() -> String {
    "public".to_owned()
}

fn default_emit_final() -> bool {
    true
}

pub(crate) struct JavaCodeGenerator;

impl CodeGenerator for JavaCodeGenerator {
    fn language(&self) -> &'static str {
        LANGUAGE
    }

    fn language_aliases(&self) -> &'static [&'static str] {
        &["jvm"]
    }

    fn default_module_name(&self) -> &'static str {
        "configforge.generated"
    }

    fn default_naming(&self) -> NamingConfig {
        NamingConfig {
            type_name: CaseStyle::PascalCase,
            enum_value: CaseStyle::ScreamingSnakeCase,
            field: CaseStyle::CamelCase,
            function: CaseStyle::CamelCase,
            constant: CaseStyle::ScreamingSnakeCase,
        }
    }

    fn default_formatting(&self) -> CodeWriterConfig {
        CodeWriterConfig::default().with_indent_width(4)
    }

    #[cfg(test)]
    fn generate(&self, input: &CodeExportInput<'_>) -> Result<Vec<super::GeneratedFile>> {
        let options = input
            .target
            .options
            .language_options::<JavaCodegenOptions>(LANGUAGE)?;
        let output_path = java_output_path(input, &options)?;
        let mut content = super::CodeWriter::new(input.target.options.formatting);
        render_java_code(self, &mut content, input, &options)?;
        Ok(vec![super::GeneratedFile {
            path: output_path,
            content: content.finish(),
        }])
    }

    fn generate_to_sink(&self, input: &CodeExportInput<'_>, sink: &ArtifactSink) -> Result<()> {
        let options = input
            .target
            .options
            .language_options::<JavaCodegenOptions>(LANGUAGE)?;
        let output_path = java_output_path(input, &options)?;
        let stream = sink.create(output_path.clone())?;
        let mut content = CheckedStreamingCodeWriter::new(stream, input.target.options.formatting);
        render_java_code(self, &mut content, input, &options)?;
        let stream = content
            .finish()
            .map_err(|source| ConfigForgeError::WriteFile {
                path: output_path,
                source,
            })?;
        stream.finish().map(|_| ())
    }
}

fn java_output_path(
    input: &CodeExportInput<'_>,
    options: &JavaCodegenOptions,
) -> Result<std::path::PathBuf> {
    let root_class_name = JavaNames::resolve_root_class_name(input, options)?;
    let file_name = match &input.target.file_name {
        Some(file_name) if std::path::Path::new(file_name).extension().is_some() => {
            let stem = std::path::Path::new(file_name)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(file_name);
            if stem != root_class_name {
                return Err(validation_error(format!(
                    "java codegen: file_name stem `{stem}` must match root_class_name `{root_class_name}`"
                )));
            }
            file_name.clone()
        }
        Some(file_name) => {
            if file_name != &root_class_name {
                return Err(validation_error(format!(
                    "java codegen: file_name `{file_name}` must match root_class_name `{root_class_name}`"
                )));
            }
            format!("{file_name}.java")
        }
        None => format!("{root_class_name}.java"),
    };
    Ok(input.target.out_dir.join(file_name))
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

fn render_java_code(
    generator: &JavaCodeGenerator,
    content: &mut impl CodeWriterLike,
    input: &CodeExportInput<'_>,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let names = JavaNames::new(input, options)?;
    let model = build_code_model(input.meta, &input.target_tags, &input.runtime_data_files)?;
    require_non_empty_index_fields(&model)?;
    validate_generated_symbols(&model, &names)?;

    write_file_header(generator, content, input);
    content.newline();
    if !names.package().is_empty() {
        content.raw_fmt(format_args!("package {};\n\n", names.package()));
    }
    content.raw("import java.util.*;\n\n");
    let root_modifier = names.root_access_modifier();
    let final_modifier = if options.emit_final { " final" } else { "" };
    let prefix = if root_modifier.is_empty() {
        final_modifier.to_owned()
    } else {
        format!("{root_modifier}{final_modifier}")
    };
    content.raw_fmt(format_args!(
        "{} class {} {{\n",
        prefix.trim_start(),
        names.root_class_name()
    ));
    content.raw("    private ");
    content.raw_fmt(format_args!("{}() {{}}\n\n", names.root_class_name()));
    render_runtime_contract(content, &model.enums, &names)?;
    render_java_types(content, &model, &names, options)?;
    render_game_data_access_interface(content, &model, &names)?;
    render_game_data_class(content, &model, &names, options)?;
    content.raw("}\n");
    Ok(())
}

fn render_runtime_contract(
    content: &mut impl CodeWriterLike,
    enums: &[CodeEnum],
    names: &JavaNames,
) -> Result<()> {
    content.raw("    public interface ConfigForgeRuntimeDataLoader {\n");
    content.raw("        ConfigForgeRuntimeDataReader open(String path);\n");
    content.raw("    }\n\n");
    content.raw("    public interface ConfigForgeRuntimeDataReader {\n");
    content.raw("        void enterRequiredField(String name);\n");
    content.raw("        boolean enterOptionalField(String name);\n");
    content.raw("        void exitField();\n");
    content.raw("        int readRowCount();\n");
    content.raw("        void enterRow(int index);\n");
    content.raw("        void exitRow();\n");
    content.raw("        int readListLength();\n");
    content.raw("        void enterListElement(int index);\n");
    content.raw("        void exitListElement();\n");
    content.raw("        long readInt();\n");
    content.raw("        double readFloat();\n");
    content.raw("        boolean readBool();\n");
    content.raw("        String readString();\n");
    content.raw("    }\n\n");
    content.raw(
        "    public static final class ConfigForgeRuntimeException extends RuntimeException {\n",
    );
    content.raw("        public ConfigForgeRuntimeException(String message) { super(message); }\n");
    content.raw("        public ConfigForgeRuntimeException(String message, Throwable cause) { super(message, cause); }\n");
    content.raw("    }\n\n");
    content.raw("    private interface ConfigForgeValueReader<T> {\n");
    content.raw("        T read(ConfigForgeRuntimeDataReader reader);\n");
    content.raw("    }\n\n");
    content.raw("    private interface ConfigForgeArrayFactory<T> {\n");
    content.raw("        T[] create(int length);\n");
    content.raw("    }\n\n");
    content.raw("    private static final class ConfigForgeRuntimeReader {\n");
    content.raw("        private ConfigForgeRuntimeReader() {}\n\n");
    content.raw("        static long readIntValue(ConfigForgeRuntimeDataReader reader) { return reader.readInt(); }\n");
    content.raw("        static double readFloatValue(ConfigForgeRuntimeDataReader reader) { return reader.readFloat(); }\n");
    content.raw("        static boolean readBoolValue(ConfigForgeRuntimeDataReader reader) { return reader.readBool(); }\n");
    content.raw("        static String readStringValue(ConfigForgeRuntimeDataReader reader) { return reader.readString(); }\n\n");
    content.raw("        static <T extends Comparable<T>> int compareValues(T left, T right) { return left.compareTo(right); }\n\n");
    for code_enum in enums {
        let type_name = names.type_name(&code_enum.name)?;
        content.raw_fmt(format_args!(
            "        static {type_name} {}(ConfigForgeRuntimeDataReader reader) {{\n",
            names.read_enum_function_name(&code_enum.name)?
        ));
        content.raw("            String value = reader.readString();\n");
        content.raw("            switch (value) {\n");
        for value in &code_enum.values {
            content.raw_fmt(format_args!(
                "                case {:?}: return {type_name}.{};\n",
                value.name,
                names.enum_value_name(&value.name)?
            ));
        }
        content.raw_fmt(format_args!(
            "                default: throw new ConfigForgeRuntimeException(\"Unknown {type_name} value '\" + value + \"'\");\n"
        ));
        content.raw("            }\n        }\n\n");
    }
    content.raw("        static <T> T readRequiredField(ConfigForgeRuntimeDataReader reader, String name, ConfigForgeValueReader<T> read) {\n");
    content.raw("            reader.enterRequiredField(name);\n");
    content.raw("            try {\n");
    content.raw("                return read.read(reader);\n");
    content.raw("            } catch (RuntimeException error) {\n");
    content.raw("                throw new ConfigForgeRuntimeException(\"Failed to read required field '\" + name + \"'\", error);\n");
    content.raw("            } finally {\n");
    content.raw("                reader.exitField();\n");
    content.raw("            }\n");
    content.raw("        }\n\n");
    content.raw("        static <T> T readOptionalField(ConfigForgeRuntimeDataReader reader, String name, ConfigForgeValueReader<T> read) {\n");
    content.raw("            if (!reader.enterOptionalField(name)) {\n");
    content.raw("                return null;\n");
    content.raw("            }\n");
    content.raw("            try {\n");
    content.raw("                return read.read(reader);\n");
    content.raw("            } catch (RuntimeException error) {\n");
    content.raw("                throw new ConfigForgeRuntimeException(\"Failed to read optional field '\" + name + \"'\", error);\n");
    content.raw("            } finally {\n");
    content.raw("                reader.exitField();\n");
    content.raw("            }\n");
    content.raw("        }\n\n");
    render_runtime_list_reader(content, "long", "readIntList", "readIntValue");
    render_runtime_list_reader(content, "double", "readFloatList", "readFloatValue");
    render_runtime_list_reader(content, "boolean", "readBoolList", "readBoolValue");
    content.raw("        static <T> T[] readObjectList(ConfigForgeRuntimeDataReader reader, ConfigForgeArrayFactory<T> factory, ConfigForgeValueReader<T> read) {\n");
    content.raw("            int length = reader.readListLength();\n");
    content.raw("            T[] values = factory.create(length);\n");
    content.raw("            for (int index = 0; index < length; index++) {\n");
    content.raw("                reader.enterListElement(index);\n");
    content.raw("                try {\n");
    content.raw("                    values[index] = read.read(reader);\n");
    content.raw("                } finally {\n");
    content.raw("                    reader.exitListElement();\n");
    content.raw("                }\n");
    content.raw("            }\n");
    content.raw("            return values;\n");
    content.raw("        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_runtime_list_reader(
    content: &mut impl CodeWriterLike,
    java_type: &str,
    method_name: &str,
    read_method: &str,
) {
    content.raw_fmt(format_args!(
        "        static {java_type}[] {method_name}(ConfigForgeRuntimeDataReader reader) {{\n"
    ));
    content.raw("            int length = reader.readListLength();\n");
    content.raw_fmt(format_args!(
        "            {java_type}[] values = new {java_type}[length];\n"
    ));
    content.raw("            for (int index = 0; index < length; index++) {\n");
    content.raw("                reader.enterListElement(index);\n");
    content.raw("                try {\n");
    content.raw_fmt(format_args!(
        "                    values[index] = {read_method}(reader);\n"
    ));
    content.raw("                } finally {\n");
    content.raw("                    reader.exitListElement();\n");
    content.raw("                }\n");
    content.raw("            }\n");
    content.raw("            return values;\n");
    content.raw("        }\n\n");
}

fn render_java_types(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    for code_enum in &model.enums {
        content.raw_fmt(format_args!(
            "    public enum {} {{\n",
            names.type_name(&code_enum.name)?
        ));
        for (index, value) in code_enum.values.iter().enumerate() {
            let suffix = if index + 1 == code_enum.values.len() {
                ";"
            } else {
                ","
            };
            content.raw_fmt(format_args!(
                "        {}{}\n",
                names.enum_value_name(&value.name)?,
                suffix
            ));
        }
        content.raw("    }\n\n");
    }

    for code_struct in &model.structs {
        render_java_class(
            content,
            &names.type_name(&code_struct.name)?,
            &code_struct.fields,
            names,
            options,
        )?;
        render_read_method(
            content,
            &names.read_type_function_name(&code_struct.name)?,
            &names.type_name(&code_struct.name)?,
            &code_struct.fields,
            names,
        )?;
    }

    for table in &model.tables {
        for fragment in &table.fragments {
            if matches!(fragment.payload, CodeFragmentPayload::Struct { .. }) {
                continue;
            }
            render_java_class(
                content,
                &names.fragment_type_name(&table.name, &fragment.name)?,
                &fragment.fields,
                names,
                options,
            )?;
            render_read_method(
                content,
                &names.read_type_function_name(&format!(
                    "{}_{}_fragment",
                    table.name, fragment.name
                ))?,
                &names.fragment_type_name(&table.name, &fragment.name)?,
                &fragment.fields,
                names,
            )?;
        }

        let mut row_fields = table.fields.clone();
        for fragment in &table.fragments {
            row_fields.push(CodeField {
                name: fragment.name.clone(),
                code_type: CodeTypeRef::Struct {
                    name: names.fragment_payload_type_name(table, fragment)?,
                },
                desc: fragment.desc.clone(),
                required: false,
            });
        }
        render_java_row_class(
            content,
            &names.row_type_name(&table.name)?,
            &row_fields,
            table,
            model,
            names,
            options,
        )?;
        render_row_read_method(content, table, names)?;

        if table.kind == CodeTableKind::Collection {
            for index in table.indexes.iter().filter(|index| index.fields.len() > 1) {
                render_compound_key_type(content, &table.name, index, names, options)?;
            }
            for index in table.indexes.iter().filter(|index| index.ordered) {
                render_ordered_entry_type(content, &table.name, index, names, options)?;
            }
            render_java_table(content, table, names, options)?;
        } else {
            render_object_loader(content, table, names)?;
        }
    }
    Ok(())
}

fn render_read_method(
    content: &mut impl CodeWriterLike,
    method_name: &str,
    type_name: &str,
    fields: &[CodeField],
    names: &JavaNames,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    private static {type_name} {method_name}(ConfigForgeRuntimeDataReader reader) {{\n"
    ));
    content.raw_fmt(format_args!("        return new {type_name}(\n"));
    for (index, field) in fields.iter().enumerate() {
        let comma = if index + 1 == fields.len() { "" } else { "," };
        content.raw_fmt(format_args!(
            "            {}{}\n",
            names.read_field_expression(field)?,
            comma
        ));
    }
    content.raw("        );\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_relation_members(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    model: &CodeModel,
    names: &JavaNames,
) -> Result<()> {
    let linked = table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
        .collect::<Vec<_>>();
    if linked.is_empty() {
        return Ok(());
    }
    content.raw("\n");
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
                content.raw_fmt(format_args!("        private int[] {cache};\n"));
            }
        }
    }
    content.raw("\n");
    let row_type = names.row_type_name(&table.name)?;
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
        let cache = names.relation_cache_field_name(foreign_key)?;
        let target_row = names.row_type_name(&foreign_key.target_table)?;
        let target_getter = names.getter_name(&foreign_key.target_table)?;
        let target_index_lookup = names.index_row_index_function_name(target_index)?;
        let args = foreign_key
            .fields
            .iter()
            .map(|field| relation_arg(field, names))
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
                    "        public {target_row} {method}(GameDataAccess data) {{\n            if ({cache} >= 0) {{\n                return data.{target_getter}().rowAt({cache});\n            }}\n            int targetIndex = data.{target_getter}().{target_index_lookup}({args});\n            if (targetIndex < 0) {{\n                throw new ConfigForgeRuntimeException(\"Missing relation target for {row_type}.{method}\");\n            }}\n            {cache} = targetIndex;\n            return data.{target_getter}().rowAt({cache});\n        }}\n\n"
                ));
            }
            CodeRelationCardinality::OptionalOne => {
                let guard = foreign_key
                    .fields
                    .iter()
                    .filter(|field| !field.required)
                    .map(|field| Ok(format!("{}() == null", names.getter_name(&field.name)?)))
                    .collect::<Result<Vec<_>>>()?
                    .join(" || ");
                content.raw_fmt(format_args!(
                    "        public {target_row} {method}(GameDataAccess data) {{\n"
                ));
                if !guard.is_empty() {
                    content.raw_fmt(format_args!(
                        "            if ({guard}) {{\n                return null;\n            }}\n"
                    ));
                }
                content.raw_fmt(format_args!(
                    "            if ({cache} >= 0) {{\n                return data.{target_getter}().rowAt({cache});\n            }}\n            int targetIndex = data.{target_getter}().{target_index_lookup}({args});\n            if (targetIndex < 0) {{\n                throw new ConfigForgeRuntimeException(\"Missing relation target for {row_type}.{method}\");\n            }}\n            {cache} = targetIndex;\n            return data.{target_getter}().rowAt({cache});\n        }}\n\n"
                ));
            }
            CodeRelationCardinality::Many => {
                let field = &foreign_key.fields[0];
                let getter = names.getter_name(&field.name)?;
                content.raw_fmt(format_args!(
                    "        public List<{target_row}> {method}(GameDataAccess data) {{\n            if ({cache} == null) {{\n                List<Integer> indexes = new ArrayList<>();\n                for (long key : {getter}()) {{\n                    int targetIndex = data.{target_getter}().{target_index_lookup}(key);\n                    if (targetIndex >= 0) {{\n                        indexes.add(targetIndex);\n                    }}\n                }}\n                {cache} = new int[indexes.size()];\n                for (int index = 0; index < {cache}.length; index++) {{\n                    {cache}[index] = indexes.get(index);\n                }}\n            }}\n            List<{target_row}> result = new ArrayList<>({cache}.length);\n            for (int targetIndex : {cache}) {{\n                result.add(data.{target_getter}().rowAt(targetIndex));\n            }}\n            return Collections.unmodifiableList(result);\n        }}\n\n"
                ));
            }
        }
    }
    Ok(())
}

fn relation_arg(field: &CodeField, names: &JavaNames) -> Result<String> {
    Ok(format!("{}()", names.getter_name(&field.name)?))
}

fn render_row_read_method(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
) -> Result<()> {
    let row_type = names.row_type_name(&table.name)?;
    let method_name = names.read_type_function_name(&format!("{}_row", table.name))?;
    content.raw_fmt(format_args!(
        "    private static {row_type} {method_name}(ConfigForgeRuntimeDataReader reader) {{\n"
    ));
    content.raw_fmt(format_args!("        return new {row_type}(\n"));
    let mut expressions = Vec::new();
    for field in &table.fields {
        expressions.push(names.read_field_expression(field)?);
    }
    for fragment in &table.fragments {
        expressions.push(format!(
            "ConfigForgeRuntimeReader.readOptionalField(reader, {:?}, {}::{})",
            fragment.name,
            names.root_class_name(),
            names.read_type_function_name(&match &fragment.payload {
                CodeFragmentPayload::InlineFields => {
                    format!("{}_{}_fragment", table.name, fragment.name)
                }
                CodeFragmentPayload::Struct { name } => name.clone(),
            })?
        ));
    }
    for (index, expression) in expressions.iter().enumerate() {
        let comma = if index + 1 == expressions.len() {
            ""
        } else {
            ","
        };
        content.raw_fmt(format_args!("            {expression}{comma}\n"));
    }
    content.raw("        );\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_java_class(
    content: &mut impl CodeWriterLike,
    class_name: &str,
    fields: &[CodeField],
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    content.raw_fmt(format_args!(
        "    public static{} class {} {{\n",
        final_modifier, class_name
    ));
    for field in fields {
        content.raw_fmt(format_args!(
            "        private final {} {};\n",
            names.java_type(field)?,
            names.field_name(&field.name)?
        ));
    }
    if !fields.is_empty() {
        content.raw("\n");
    }
    content.raw_fmt(format_args!("        private {}(", class_name));
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            content.raw(", ");
        }
        content.raw_fmt(format_args!(
            "{} {}",
            names.java_type(field)?,
            names.field_name(&field.name)?
        ));
    }
    content.raw(") {\n");
    for field in fields {
        let field_name = names.field_name(&field.name)?;
        content.raw_fmt(format_args!(
            "            this.{field_name} = {field_name};\n"
        ));
    }
    content.raw("        }\n");
    for field in fields {
        content.raw("\n");
        content.raw_fmt(format_args!(
            "        public {} {}() {{ return {}; }}\n",
            names.java_type(field)?,
            names.getter_name(&field.name)?,
            names.field_name(&field.name)?
        ));
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_java_row_class(
    content: &mut impl CodeWriterLike,
    class_name: &str,
    fields: &[CodeField],
    table: &CodeTable,
    model: &CodeModel,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    content.raw_fmt(format_args!(
        "    public static{} class {} {{\n",
        final_modifier, class_name
    ));
    for field in fields {
        content.raw_fmt(format_args!(
            "        private final {} {};\n",
            names.java_type(field)?,
            names.field_name(&field.name)?
        ));
    }
    render_relation_members(content, table, model, names)?;
    if !fields.is_empty() {
        content.raw("\n");
    }
    content.raw_fmt(format_args!("        private {}(", class_name));
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            content.raw(", ");
        }
        content.raw_fmt(format_args!(
            "{} {}",
            names.java_type(field)?,
            names.field_name(&field.name)?
        ));
    }
    content.raw(") {\n");
    for field in fields {
        let field_name = names.field_name(&field.name)?;
        content.raw_fmt(format_args!(
            "            this.{field_name} = {field_name};\n"
        ));
    }
    content.raw("        }\n");
    for field in fields {
        content.raw("\n");
        content.raw_fmt(format_args!(
            "        public {} {}() {{ return {}; }}\n",
            names.java_type(field)?,
            names.getter_name(&field.name)?,
            names.field_name(&field.name)?
        ));
    }
    content.raw("    }\n\n");
    Ok(())
}

fn render_java_table(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    let table_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "    public static{} class {} {{\n",
        final_modifier, table_type
    ));
    content.raw_fmt(format_args!(
        "        public static final String DATA_FILE = \"{}\";\n",
        table.data_file
    ));
    content.raw_fmt(format_args!(
        "        private final {}[] rows;\n\n",
        row_type
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            let entry_type = names.ordered_entry_type_name(&table.name, index)?;
            content.raw_fmt(format_args!(
                "        private final {entry_type}[] {storage};\n"
            ));
        } else if index.unique {
            content.raw_fmt(format_args!(
                "        private final Map<{key_type}, Integer> {storage};\n"
            ));
        } else {
            content.raw_fmt(format_args!(
                "        private final Map<{key_type}, int[]> {storage};\n"
            ));
        }
    }
    if !table.indexes.is_empty() {
        content.raw("\n");
    }
    content.raw_fmt(format_args!(
        "        private {}({}[] rows",
        table_type, row_type
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            let entry_type = names.ordered_entry_type_name(&table.name, index)?;
            content.raw_fmt(format_args!(", {entry_type}[] {storage}"));
        } else if index.unique {
            content.raw_fmt(format_args!(", Map<{key_type}, Integer> {storage}"));
        } else {
            content.raw_fmt(format_args!(", Map<{key_type}, int[]> {storage}"));
        }
    }
    content.raw(") {\n            this.rows = rows;\n");
    for index in &table.indexes {
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!("            this.{storage} = {storage};\n"));
    }
    content.raw("        }\n\n");
    content.raw_fmt(format_args!(
        "        public static {table_type} load(ConfigForgeRuntimeDataLoader loader) {{\n"
    ));
    content.raw("            ConfigForgeRuntimeDataReader reader = loader.open(DATA_FILE);\n");
    content.raw("            try {\n");
    content.raw("                int rowCount = reader.readRowCount();\n");
    content.raw_fmt(format_args!(
        "                {row_type}[] rows = new {row_type}[rowCount];\n"
    ));
    content.raw("                for (int rowIndex = 0; rowIndex < rowCount; rowIndex++) {\n");
    content.raw("                    reader.enterRow(rowIndex);\n");
    content.raw("                    try {\n");
    content.raw_fmt(format_args!(
        "                        rows[rowIndex] = {}(reader);\n",
        names.read_type_function_name(&format!("{}_row", table.name))?
    ));
    content.raw("                    } finally {\n");
    content.raw("                        reader.exitRow();\n");
    content.raw("                    }\n");
    content.raw("                }\n");
    content.raw_fmt(format_args!("                return fromRows(rows);\n"));
    content.raw("            } catch (RuntimeException error) {\n");
    content.raw("                throw new ConfigForgeRuntimeException(\"Failed to load table '\" + DATA_FILE + \"'\", error);\n");
    content.raw("            }\n");
    content.raw("        }\n\n");
    content.raw("        public int size() { return rows.length; }\n");
    content.raw_fmt(format_args!(
        "        public {} rowAt(int index) {{ return rows[index]; }}\n",
        row_type
    ));
    render_table_from_rows_method(content, table, names)?;
    render_index_lookup_methods(content, table, names)?;
    render_table_link_method(content, table, names)?;
    content.raw("    }\n\n");
    Ok(())
}

fn render_table_link_method(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
) -> Result<()> {
    let linked = table
        .foreign_keys
        .iter()
        .filter(|foreign_key| foreign_key.linked_field.is_some())
        .collect::<Vec<_>>();
    if linked.is_empty() {
        return Ok(());
    }
    content.raw("\n        public void link(GameDataAccess data) {\n");
    for foreign_key in linked {
        let method = names.relation_function_name(foreign_key)?;
        content.raw_fmt(format_args!(
            "            for ({} row : rows) {{\n                row.{method}(data);\n            }}\n",
            names.row_type_name(&table.name)?
        ));
    }
    content.raw("        }\n");
    Ok(())
}

fn render_compound_key_type(
    content: &mut impl CodeWriterLike,
    table_name: &str,
    index: &CodeIndex,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    let key_type = names.compound_key_type_name(table_name, index)?;
    content.raw_fmt(format_args!(
        "    private static{} class {key_type} implements Comparable<{key_type}> {{\n",
        final_modifier
    ));
    for field in &index.fields {
        content.raw_fmt(format_args!(
            "        private final {} {};\n",
            names.java_type_ref(&field.code_type, true)?,
            names.field_name(&field.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "\n        private {key_type}({}) {{\n",
        names.index_parameters(index)?
    ));
    for field in &index.fields {
        let field_name = names.field_name(&field.name)?;
        let local_name = names.local_name(&field.name)?;
        content.raw_fmt(format_args!(
            "            this.{field_name} = {local_name};\n"
        ));
    }
    content.raw("        }\n\n");
    content.raw("        @Override\n");
    content.raw("        public boolean equals(Object other) {\n");
    content.raw_fmt(format_args!(
        "            if (!(other instanceof {key_type})) {{ return false; }}\n"
    ));
    content.raw_fmt(format_args!(
        "            {key_type} key = ({key_type}) other;\n"
    ));
    let equals_expr = index
        .fields
        .iter()
        .map(|field| {
            let field_name = names.field_name(&field.name)?;
            Ok(format!("Objects.equals({field_name}, key.{field_name})"))
        })
        .collect::<Result<Vec<_>>>()?
        .join(" && ");
    content.raw_fmt(format_args!("            return {equals_expr};\n"));
    content.raw("        }\n\n");
    content.raw("        @Override\n");
    content.raw("        public int hashCode() {\n");
    content.raw_fmt(format_args!(
        "            return Objects.hash({});\n",
        index
            .fields
            .iter()
            .map(|field| names.field_name(&field.name))
            .collect::<Result<Vec<_>>>()?
            .join(", ")
    ));
    content.raw("        }\n");
    content.raw("\n        @Override\n");
    content.raw_fmt(format_args!(
        "        public int compareTo({key_type} other) {{\n"
    ));
    for field in &index.fields {
        let field_name = names.field_name(&field.name)?;
        let local_name = names.local_name(&field.name)?;
        content.raw_fmt(format_args!(
            "            int {local_name}Comparison = ConfigForgeRuntimeReader.compareValues({field_name}, other.{field_name});\n"
        ));
        content.raw_fmt(format_args!(
            "            if ({local_name}Comparison != 0) {{ return {local_name}Comparison; }}\n"
        ));
    }
    content.raw("            return 0;\n");
    content.raw("        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_ordered_entry_type(
    content: &mut impl CodeWriterLike,
    table_name: &str,
    index: &CodeIndex,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    let entry_type = names.ordered_entry_type_name(table_name, index)?;
    let key_type = names.index_key_type(table_name, index)?;
    content.raw_fmt(format_args!(
        "    private static{} class {entry_type} {{\n",
        final_modifier
    ));
    content.raw_fmt(format_args!("        private final {key_type} key;\n"));
    content.raw("        private final int rowIndex;\n\n");
    content.raw_fmt(format_args!(
        "        private {entry_type}({key_type} key, int rowIndex) {{\n"
    ));
    content.raw("            this.key = key;\n");
    content.raw("            this.rowIndex = rowIndex;\n");
    content.raw("        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_table_from_rows_method(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
) -> Result<()> {
    let table_type = names.repository_type_name(&table.name)?;
    let row_type = names.row_type_name(&table.name)?;
    content.raw_fmt(format_args!(
        "\n        private static {table_type} fromRows({row_type}[] rows) {{\n"
    ));
    for index in &table.indexes {
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        if index.ordered {
            let entry_type = names.ordered_entry_type_name(&table.name, index)?;
            content.raw_fmt(format_args!(
                "            {entry_type}[] {storage} = new {entry_type}[rows.length];\n"
            ));
        } else if index.unique {
            content.raw_fmt(format_args!(
                "            Map<{key_type}, Integer> {storage} = new HashMap<>();\n"
            ));
        } else {
            content.raw_fmt(format_args!(
                "            Map<{key_type}, List<Integer>> {storage}Builder = new HashMap<>();\n"
            ));
        }
    }
    if !table.indexes.is_empty() {
        content.raw("            for (int rowIndex = 0; rowIndex < rows.length; rowIndex++) {\n");
        content.raw("                ");
        content.raw_fmt(format_args!("{} row = rows[rowIndex];\n", row_type));
        for index in &table.indexes {
            let key_expr = names.index_key_from_row(&table.name, index, "row")?;
            let storage = names.index_storage_name(index)?;
            if index.ordered {
                let entry_type = names.ordered_entry_type_name(&table.name, index)?;
                content.raw_fmt(format_args!(
                    "                {storage}[rowIndex] = new {entry_type}({key_expr}, rowIndex);\n"
                ));
            } else if index.unique {
                content.raw_fmt(format_args!(
                    "                {} {storage}Key = {key_expr};\n",
                    names.index_key_type(&table.name, index)?
                ));
                content.raw_fmt(format_args!(
                    "                if ({storage}.containsKey({storage}Key)) {{\n                    throw new ConfigForgeRuntimeException(\"Duplicate unique index key for {table_type}.{index_name}\");\n                }}\n",
                    index_name = index.name
                ));
                content.raw_fmt(format_args!(
                    "                {storage}.put({storage}Key, rowIndex);\n"
                ));
            } else {
                content.raw_fmt(format_args!(
                    "                {} {storage}Key = {key_expr};\n",
                    names.index_key_type(&table.name, index)?
                ));
                content.raw_fmt(format_args!(
                    "                List<Integer> {storage}Rows = {storage}Builder.get({storage}Key);\n"
                ));
                content.raw_fmt(format_args!(
                    "                if ({storage}Rows == null) {{\n                    {storage}Rows = new ArrayList<>();\n                    {storage}Builder.put({storage}Key, {storage}Rows);\n                }}\n"
                ));
                content.raw_fmt(format_args!(
                    "                {storage}Rows.add(rowIndex);\n"
                ));
            }
        }
        content.raw("            }\n");
    }
    for index in table.indexes.iter().filter(|index| index.ordered) {
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!(
            "            Arrays.sort({storage}, (left, right) -> ConfigForgeRuntimeReader.compareValues(left.key, right.key));\n"
        ));
        if index.unique {
            content.raw_fmt(format_args!(
                "            for (int index = 1; index < {storage}.length; index++) {{\n                if (ConfigForgeRuntimeReader.compareValues({storage}[index - 1].key, {storage}[index].key) == 0) {{\n                    throw new ConfigForgeRuntimeException(\"Duplicate unique index key for {table_type}.{index_name}\");\n                }}\n            }}\n",
                index_name = index.name
            ));
        }
    }
    for index in table.indexes.iter().filter(|index| !index.unique) {
        if index.ordered {
            continue;
        }
        let key_type = names.index_key_type(&table.name, index)?;
        let storage = names.index_storage_name(index)?;
        content.raw_fmt(format_args!(
            "            Map<{key_type}, int[]> {storage} = new HashMap<>();\n"
        ));
        content.raw_fmt(format_args!(
            "            for (Map.Entry<{key_type}, List<Integer>> entry : {storage}Builder.entrySet()) {{\n"
        ));
        content.raw("                int[] rowIndexes = new int[entry.getValue().size()];\n");
        content.raw("                for (int index = 0; index < rowIndexes.length; index++) {\n");
        content.raw("                    rowIndexes[index] = entry.getValue().get(index);\n");
        content.raw("                }\n");
        content.raw_fmt(format_args!(
            "                {storage}.put(entry.getKey(), rowIndexes);\n"
        ));
        content.raw("            }\n");
    }
    content.raw_fmt(format_args!("            return new {table_type}(rows"));
    for index in &table.indexes {
        content.raw_fmt(format_args!(", {}", names.index_storage_name(index)?));
    }
    content.raw(");\n");
    content.raw("        }\n");
    Ok(())
}

fn render_index_lookup_methods(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
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
            let entry_type = names.ordered_entry_type_name(&table.name, index)?;
            if index.unique {
                let row_index_method = names.index_row_index_function_name(index)?;
                content.raw_fmt(format_args!(
                    "\n        private int {row_index_method}({parameters}) {{\n            {key_type} key = {key};\n            int index = lowerBound{entry_type}({storage}, key);\n            if (index < {storage}.length && ConfigForgeRuntimeReader.compareValues({storage}[index].key, key) == 0) {{\n                return {storage}[index].rowIndex;\n            }}\n            return -1;\n        }}\n\n        public {row_type} {method}({parameters}) {{\n            int rowIndex = {row_index_method}({args});\n            return rowIndex >= 0 ? rows[rowIndex] : null;\n        }}\n",
                    args = index
                        .fields
                        .iter()
                        .map(|field| names.local_name(&field.name))
                        .collect::<Result<Vec<_>>>()?
                        .join(", "),
                    key_type = names.index_key_type(&table.name, index)?
                ));
            } else {
                content.raw_fmt(format_args!(
                    "\n        public List<{row_type}> {method}({parameters}) {{\n            {key_type} key = {key};\n            int start = lowerBound{entry_type}({storage}, key);\n            int end = upperBound{entry_type}({storage}, key);\n            return rowsFrom{entry_type}Range({storage}, start, end);\n        }}\n",
                    key_type = names.index_key_type(&table.name, index)?
                ));
            }
            content.raw_fmt(format_args!(
                "\n        public int {lower_method}({parameters}) {{\n            return lowerBound{entry_type}({storage}, {key});\n        }}\n\n        public int {upper_method}({parameters}) {{\n            return upperBound{entry_type}({storage}, {key});\n        }}\n\n        public List<{row_type}> {range_method}({min_parameters}, {max_parameters}) {{\n            int start = lowerBound{entry_type}({storage}, {key_min});\n            int end = upperBound{entry_type}({storage}, {key_max});\n            return rowsFrom{entry_type}Range({storage}, start, end);\n        }}\n",
                min_parameters = names.index_parameters_with_suffix(index, "Min")?,
                max_parameters = names.index_parameters_with_suffix(index, "Max")?,
                key_min = names.index_key_from_parameter_suffix(&table.name, index, "Min")?,
                key_max = names.index_key_from_parameter_suffix(&table.name, index, "Max")?
            ));
            render_ordered_search_helpers(
                content,
                &row_type,
                &entry_type,
                &names.index_key_type(&table.name, index)?,
                &storage,
            )?;
        } else if index.unique {
            let row_index_method = names.index_row_index_function_name(index)?;
            content.raw_fmt(format_args!(
                "\n        private int {row_index_method}({parameters}) {{\n            Integer rowIndex = {storage}.get({key});\n            return rowIndex != null ? rowIndex : -1;\n        }}\n\n        public {row_type} {method}({parameters}) {{\n            int rowIndex = {row_index_method}({args});\n            return rowIndex >= 0 ? rows[rowIndex] : null;\n        }}\n",
                args = index
                    .fields
                    .iter()
                    .map(|field| names.local_name(&field.name))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ));
        } else {
            content.raw_fmt(format_args!(
                "\n        public List<{row_type}> {method}({parameters}) {{\n            int[] rowIndexes = {storage}.get({key});\n            if (rowIndexes == null) {{\n                return Collections.emptyList();\n            }}\n            List<{row_type}> result = new ArrayList<>(rowIndexes.length);\n            for (int rowIndex : rowIndexes) {{\n                result.add(rows[rowIndex]);\n            }}\n            return Collections.unmodifiableList(result);\n        }}\n"
            ));
        }
    }
    Ok(())
}

fn render_ordered_search_helpers(
    content: &mut impl CodeWriterLike,
    row_type: &str,
    entry_type: &str,
    key_type: &str,
    storage: &str,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "\n        private static int lowerBound{entry_type}({entry_type}[] entries, {key_type} key) {{\n            int low = 0;\n            int high = entries.length;\n            while (low < high) {{\n                int mid = low + ((high - low) / 2);\n                if (entries[mid].key.compareTo(key) < 0) {{\n                    low = mid + 1;\n                }} else {{\n                    high = mid;\n                }}\n            }}\n            return low;\n        }}\n\n        private static int upperBound{entry_type}({entry_type}[] entries, {key_type} key) {{\n            int low = 0;\n            int high = entries.length;\n            while (low < high) {{\n                int mid = low + ((high - low) / 2);\n                if (entries[mid].key.compareTo(key) <= 0) {{\n                    low = mid + 1;\n                }} else {{\n                    high = mid;\n                }}\n            }}\n            return low;\n        }}\n\n        private List<{row_type}> rowsFrom{entry_type}Range({entry_type}[] {storage}, int start, int end) {{\n            int length = Math.max(0, end - start);\n            List<{row_type}> result = new ArrayList<>(length);\n            for (int offset = 0; offset < length; offset++) {{\n                result.add(rows[{storage}[start + offset].rowIndex]);\n            }}\n            return Collections.unmodifiableList(result);\n        }}\n"
    ));
    Ok(())
}

fn render_object_loader(
    content: &mut impl CodeWriterLike,
    table: &CodeTable,
    names: &JavaNames,
) -> Result<()> {
    content.raw_fmt(format_args!(
        "    private static {} load{}(ConfigForgeRuntimeDataLoader loader) {{\n",
        names.row_type_name(&table.name)?,
        names.type_name(&table.name)?
    ));
    content.raw_fmt(format_args!(
        "        ConfigForgeRuntimeDataReader reader = loader.open({:?});\n",
        table.data_file
    ));
    content.raw("        try {\n");
    content.raw_fmt(format_args!(
        "            return {}(reader);\n",
        names.read_type_function_name(&format!("{}_row", table.name))?
    ));
    content.raw("        } catch (RuntimeException error) {\n");
    content.raw_fmt(format_args!(
        "            throw new ConfigForgeRuntimeException(\"Failed to load object '{}'\", error);\n",
        table.data_file
    ));
    content.raw("        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_game_data_class(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &JavaNames,
    options: &JavaCodegenOptions,
) -> Result<()> {
    let final_modifier = if options.emit_final { " final" } else { "" };
    content.raw_fmt(format_args!(
        "    public static{} class {} implements GameDataAccess {{\n",
        final_modifier,
        names.game_data_class_name()
    ));
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "        private final {} {};\n",
            java_table_data_type(table, names)?,
            names.field_name(&table.name)?
        ));
    }
    content.raw_fmt(format_args!(
        "\n        private {}(\n",
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
            java_table_data_type(table, names)?,
            names.local_name(&table.name)?,
            comma
        ));
    }
    content.raw("        ) {\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "            this.{} = {};\n",
            names.field_name(&table.name)?,
            names.local_name(&table.name)?
        ));
    }
    content.raw("        }\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "\n        @Override\n        public {} {}() {{ return {}; }}\n",
            java_table_data_type(table, names)?,
            names.getter_name(&table.name)?,
            names.field_name(&table.name)?
        ));
    }
    content.raw("\n        public void linkAll() {\n");
    for table in model.tables.iter().filter(|table| {
        table
            .foreign_keys
            .iter()
            .any(|foreign_key| foreign_key.linked_field.is_some())
    }) {
        match table.kind {
            CodeTableKind::Collection => content.raw_fmt(format_args!(
                "            {}.link(this);\n",
                names.field_name(&table.name)?
            )),
            CodeTableKind::Object => {
                for foreign_key in table
                    .foreign_keys
                    .iter()
                    .filter(|foreign_key| foreign_key.linked_field.is_some())
                {
                    content.raw_fmt(format_args!(
                        "            {}.{}(this);\n",
                        names.field_name(&table.name)?,
                        names.relation_function_name(foreign_key)?
                    ));
                }
            }
        }
    }
    content.raw("        }\n");
    content.raw_fmt(format_args!(
        "\n        private static {} fromLoaded(\n",
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
            java_table_data_type(table, names)?,
            names.local_name(&table.name)?,
            comma
        ));
    }
    content.raw_fmt(format_args!(
        "        ) {{\n            {} data = new {}(\n",
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
            names.local_name(&table.name)?,
            comma
        ));
    }
    content
        .raw("            );\n            data.linkAll();\n            return data;\n        }\n");
    content.raw_fmt(format_args!(
        "\n        public static {} load(ConfigForgeRuntimeDataLoader loader) {{\n",
        names.game_data_class_name()
    ));
    content.raw("            return fromLoaded(\n");
    for (index, table) in model.tables.iter().enumerate() {
        let comma = if index + 1 == model.tables.len() {
            ""
        } else {
            ","
        };
        let load_expr = match table.kind {
            CodeTableKind::Collection => {
                format!("{}.load(loader)", names.repository_type_name(&table.name)?)
            }
            CodeTableKind::Object => format!("load{}(loader)", names.type_name(&table.name)?),
        };
        content.raw_fmt(format_args!("                {load_expr}{comma}\n"));
    }
    content.raw("            );\n");
    content.raw("        }\n");
    content.raw("    }\n\n");
    Ok(())
}

fn render_game_data_access_interface(
    content: &mut impl CodeWriterLike,
    model: &CodeModel,
    names: &JavaNames,
) -> Result<()> {
    content.raw("    public interface GameDataAccess {\n");
    for table in &model.tables {
        content.raw_fmt(format_args!(
            "        {} {}();\n",
            java_table_data_type(table, names)?,
            names.getter_name(&table.name)?
        ));
    }
    content.raw("    }\n\n");
    Ok(())
}

fn java_table_data_type(table: &CodeTable, names: &JavaNames) -> Result<String> {
    match table.kind {
        CodeTableKind::Object => names.row_type_name(&table.name),
        CodeTableKind::Collection => names.repository_type_name(&table.name),
    }
}

pub(super) fn validation_error(message: String) -> ConfigForgeError {
    ConfigForgeError::Validation(message)
}
