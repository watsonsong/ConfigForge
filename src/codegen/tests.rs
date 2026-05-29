// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ConfigForgeError,
    exporter::RuntimeDataFile,
    meta::{MetaModel, TargetTags},
};

use super::*;

pub(in crate::codegen) fn linked_fixture() -> MetaModel {
    MetaModel::from_toml_str(
        r#"
            [[enums]]
            name = "ItemRarity"
            desc = "Item rarity."

            [[enums.values]]
            name = "common"

            [[enums.values]]
            name = "rare"

            [[structs]]
            name = "RewardInfo"

            [[structs.fields]]
            name = "count"
            type = "int"
            required = true

            [[tables]]
            name = "items"
            desc = "Item table."
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            required = true
            unique = true

            [[tables.fields]]
            name = "display_name"
            type = "string"
            required = true

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"
            required = true

            [[tables.fields]]
            name = "internal_note"
            type = "string"
            export = false

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables.indexes]]
            name = "by_rarity"
            fields = ["rarity"]
            unique = false
        "#,
    )
    .expect("fixture meta should parse")
}

pub(in crate::codegen) fn input_for<'a>(
    meta: &'a MetaModel,
    language: &str,
) -> CodeExportInput<'a> {
    let linked = meta.link().expect("fixture meta should link");
    let linked = Box::leak(Box::new(linked));
    CodeExportInput {
        meta: linked,
        target_tags: TargetTags::empty(),
        project: CodeExportProject {
            name: Some("demo".to_owned()),
            desc: Some("Demo config".to_owned()),
            copyright: Some("Copyright Demo".to_owned()),
        },
        target: CodeExportTarget {
            language: language.to_owned(),
            out_dir: PathBuf::from("out"),
            module_name: "generated".to_owned(),
            namespace: None,
            options: CodegenOptions::for_language(language).expect("built-in language options"),
        },
        runtime_data_files: vec![RuntimeDataFile {
            table_name: "items".to_owned(),
            path: "items.json".to_owned(),
            format: "json".to_owned(),
        }],
        generated_at: "2026-05-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn builds_code_model_with_types_tables_and_indexes() {
    let meta = linked_fixture();
    let input = input_for(&meta, RustCodeGenerator.language());

    let model =
        build_code_model(input.meta, &input.target_tags, &input.runtime_data_files).expect("model");

    assert_eq!(model.enums[0].name, "ItemRarity");
    assert_eq!(model.structs[0].name, "RewardInfo");
    assert_eq!(model.tables[0].name, "items");
    assert_eq!(model.tables[0].kind, CodeTableKind::Collection);
    assert!(
        model.tables[0]
            .fields
            .iter()
            .any(|field| field.name == "display_name")
    );
    assert!(
        !model.tables[0]
            .fields
            .iter()
            .any(|field| field.name == "internal_note")
    );
    assert!(
        model.tables[0]
            .indexes
            .iter()
            .any(|index| index.name == "by_id" && index.unique)
    );
    assert!(
        model.tables[0]
            .indexes
            .iter()
            .any(|index| index.name == "by_rarity" && !index.unique)
    );
    assert_eq!(model.tables[0].data_file, "items");
}

#[test]
fn code_model_marks_object_tables_and_skips_indexes() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "game_config"
            kind = "object"
            source = "game_config.csv"

            [[tables.fields]]
            name = "max_level"
            type = "int"

            [[tables.fields]]
            name = "client_note"
            type = "string"
            export_when = { any = ["client"] }
        "#,
    )
    .expect("meta should load");
    let input = input_for(&meta, RustCodeGenerator.language());

    let model = build_code_model(input.meta, &TargetTags::new(["client"]), &[]).expect("model");

    assert_eq!(model.tables[0].name, "game_config");
    assert_eq!(model.tables[0].kind, CodeTableKind::Object);
    assert!(model.tables[0].indexes.is_empty());
    assert!(model.tables[0].foreign_keys.is_empty());
    assert_eq!(model.tables[0].data_file, "game_config");
}

#[test]
fn code_model_includes_fragment_structs_and_optional_row_members() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "AttributeSet"

            [[structs.fields]]
            name = "attack"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fragments]]
            name = "weapon"

            [[tables.fragments.fields]]
            name = "power"
            type = "int"

            [[tables.fragments.fields]]
            name = "internal_note"
            type = "string"
            export = false

            [[tables.fragments]]
            name = "attributes"
            type = "AttributeSet"

            [[tables.fragments]]
            name = "client_marker"
            export_when = { any = ["client"] }

            [[tables.fragments.fields]]
            name = "server_note"
            type = "string"
            export_when = { any = ["server"] }

            [[tables.fragments]]
            name = "server_marker"
            export_when = { any = ["server"] }

            [[tables.fragments.fields]]
            name = "value"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let input = input_for(&meta, RustCodeGenerator.language());

    let model = build_code_model(input.meta, &TargetTags::empty(), &input.runtime_data_files)
        .expect("model should build");
    assert_eq!(model.tables[0].fragments[0].name, "weapon");
    assert!(
        model.tables[0].fragments[0]
            .fields
            .iter()
            .any(|field| field.name == "power")
    );
    assert!(
        !model.tables[0].fragments[0]
            .fields
            .iter()
            .any(|field| field.name == "internal_note")
    );
    let attributes = model.tables[0]
        .fragments
        .iter()
        .find(|fragment| fragment.name == "attributes")
        .expect("typed fragment should be generated");
    assert_eq!(
        attributes.payload,
        CodeFragmentPayload::Struct {
            name: "AttributeSet".to_owned()
        }
    );

    let client_model = build_code_model(input.meta, &TargetTags::new(["client"]), &[])
        .expect("client model should build");
    let client_marker = client_model.tables[0]
        .fragments
        .iter()
        .find(|fragment| fragment.name == "client_marker")
        .expect("client marker should be generated");
    assert!(client_marker.fields.is_empty());
    assert!(
        !client_model.tables[0]
            .fragments
            .iter()
            .any(|fragment| fragment.name == "server_marker")
    );
}

#[test]
fn code_model_uses_logical_table_name_when_no_runtime_file_is_available() {
    let meta = linked_fixture();
    let linked = meta.link().expect("fixture meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");

    assert_eq!(model.tables[0].data_file, "items");
}

#[test]
fn code_model_includes_visible_foreign_keys() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            required = false
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");
    let monsters = model
        .tables
        .iter()
        .find(|table| table.name == "monsters")
        .expect("monsters table should exist");
    assert_eq!(monsters.foreign_keys.len(), 1);
    assert_eq!(monsters.foreign_keys[0].name, "drop_item");
    assert_eq!(monsters.foreign_keys[0].target_table, "items");
    assert_eq!(monsters.foreign_keys[0].target_index, "by_id");
    assert!(monsters.foreign_keys[0].allow_empty);
}

#[test]
fn code_model_rejects_visible_foreign_key_to_hidden_target_index() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            polymorphic = true
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true
            export_when = { any = ["server"] }

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("target index `items.by_id` which is not exported")
    );
}

#[test]
fn code_model_skips_foreign_key_when_export_when_does_not_match() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items", export_when = { any = ["server"] } }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::new(["client"]), &[]).expect("model");
    let monsters = model
        .tables
        .iter()
        .find(|table| table.name == "monsters")
        .expect("monsters table should exist");
    assert!(monsters.foreign_keys.is_empty());
}

#[test]
fn code_model_rejects_visible_foreign_key_to_hidden_source_field() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.indexes]]
            name = "by_id"
            fields = ["id"]
            unique = true
            primary = true

            [[tables]]
            name = "monsters"
            source = "monsters.csv"

            [[tables.fields]]
            name = "drop_item_id"
            type = "int"
            export_when = { any = ["server"] }
            foreign_key = { name = "drop_item", target_table = "items" }
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("non-exported source field `drop_item_id`")
    );
}

#[test]
fn code_model_rejects_indexes_referencing_non_exported_fields() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "internal_region"
            type = "string"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_id_region"
            fields = ["id", "internal_region"]
            unique = true
            primary = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let error = build_code_model(&linked, &TargetTags::new(["client"]), &[]).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("primary index `by_id_region` references non-exported field `internal_region`"))
    );
}

#[test]
fn code_model_filters_tables_fields_and_indexes_by_target_tags() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"

            [[tables.fields]]
            name = "icon"
            type = "asset"
            export_when = { any = ["client"] }

            [[tables.fields]]
            name = "drop_weight"
            type = "int"
            export_when = { any = ["server"] }

            [[tables.indexes]]
            name = "by_drop_weight"
            fields = ["drop_weight"]
            unique = false

            [[tables]]
            name = "server_only"
            source = "server_only.csv"
            export_when = { any = ["server"] }

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");

    let model = build_code_model(&linked, &TargetTags::new(["client"]), &[]).expect("model");

    assert!(model.tables.iter().any(|table| table.name == "items"));
    assert!(!model.tables.iter().any(|table| table.name == "server_only"));
    let items = model
        .tables
        .iter()
        .find(|table| table.name == "items")
        .expect("items table should exist");
    assert!(items.fields.iter().any(|field| field.name == "icon"));
    assert!(!items.fields.iter().any(|field| field.name == "drop_weight"));
    assert!(
        !items
            .indexes
            .iter()
            .any(|index| index.name == "by_drop_weight")
    );
}

#[test]
fn code_model_rejects_unsupported_index_key_types() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "ratio"
            type = "float"

            [[tables.indexes]]
            name = "by_ratio"
            fields = ["ratio"]
            unique = true
        "#,
    )
    .expect("meta should load");
    let linked = meta.link().expect("meta should link");
    let error = build_code_model(&linked, &TargetTags::empty(), &[]).unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::Validation(message) if message.contains("unsupported index key type"))
    );
}

pub(in crate::codegen) fn assert_duplicate_generated_identifier(
    result: crate::Result<Vec<GeneratedFile>>,
    language: &str,
    identifier: &str,
    scope: &str,
) {
    let error = result.expect_err("generator should reject duplicate generated symbols");
    assert!(
        matches!(&error, ConfigForgeError::Validation(message) if message.contains(language) && message.contains(identifier) && message.contains(scope)),
        "unexpected error: {error}"
    );
}

#[test]
fn naming_handles_reserved_words_and_cases() {
    assert_eq!(
        crate::codegen::naming::pascal_case("item-table"),
        "ItemTable"
    );
    assert_eq!(crate::codegen::naming::snake_case("type"), "type");
    assert_eq!(
        crate::codegen::naming::make_identifier(
            "type",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::codegen::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "type_"
    );
    assert!(crate::codegen::naming::is_rust_keyword("gen"));
    assert_eq!(
        crate::codegen::naming::make_identifier(
            "gen",
            CaseStyle::SnakeCase,
            KeywordConflictPolicy::SuffixUnderscore,
            crate::codegen::naming::is_rust_keyword,
            |identifier| Some(format!("r#{identifier}")),
        )
        .expect("identifier"),
        "gen_"
    );
    assert_eq!(crate::codegen::naming::camel_case("item-name"), "itemName");
}

#[test]
fn parses_codegen_task_config_and_merges_naming_defaults() {
    let config: CodegenTaskConfig = toml::from_str(
        r#"
            language = "typescript"
            output = "Generated/TS"
            file_header = "// {{project.name}} {{datetime}}"
            keyword_conflict = "error"

            [naming]
            field = "snake_case"

            [formatting]
            indent_style = "tab"
            indent_width = 8
            trailing_blank_line = true

            [typescript]
            readonly_fields = true
        "#,
    )
    .expect("config should parse");

    let target = CodeExportTarget::from_task(config, &TypeScriptCodeGenerator).expect("target");

    assert_eq!(target.language, TypeScriptCodeGenerator.language());
    assert_eq!(target.out_dir, PathBuf::from("Generated/TS"));
    assert_eq!(
        target.options.keyword_conflict,
        KeywordConflictPolicy::Error
    );
    assert_eq!(target.options.naming.type_name, CaseStyle::PascalCase);
    assert_eq!(target.options.naming.field, CaseStyle::SnakeCase);
    assert_eq!(target.options.naming.function, CaseStyle::CamelCase);
    assert_eq!(target.options.formatting.indent_style, IndentStyle::Tab);
    assert_eq!(target.options.formatting.indent_width, 8);
    assert!(target.options.formatting.trailing_newline);
    assert!(target.options.formatting.trailing_blank_line);
    assert_eq!(
        target
            .options
            .language_options::<TypeScriptCodegenOptions>(TypeScriptCodeGenerator.language())
            .expect("typescript options should parse"),
        TypeScriptCodegenOptions {
            readonly_fields: true,
        }
    );
}

#[test]
fn builtin_generators_use_language_specific_formatting_defaults() {
    assert_eq!(
        CodegenOptions::for_language(RustCodeGenerator.language())
            .expect("rust options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(CppCodeGenerator.language())
            .expect("cpp options")
            .formatting
            .indent_width,
        4
    );
    assert_eq!(
        CodegenOptions::for_language(TypeScriptCodeGenerator.language())
            .expect("typescript options")
            .formatting
            .indent_width,
        2
    );
}

#[test]
fn generated_content_respects_trailing_blank_line_option() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, TypeScriptCodeGenerator.language());
    input.target.options.formatting.trailing_blank_line = true;

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code should generate");

    assert!(files[0].content.ends_with("\n\n"));
}

#[test]
fn write_file_header_replaces_project_variables_and_datetime() {
    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let mut input = input_for(&meta, RustCodeGenerator.language());
    input.target.options.file_header = Some(
        "// {{project.name}} | {{project.desc}} | {{project.copyright}} | {{datetime}}".to_owned(),
    );
    let mut writer = CodeWriter::new(input.target.options.formatting);
    RustCodeGenerator.write_file_header(&mut writer, &input);

    assert_eq!(
        writer.finish(),
        "// demo | Demo config | Copyright Demo | 2026-05-17T00:00:00Z\n"
    );
}

#[test]
fn default_file_header_uses_generator_comment_prefix() {
    struct HashCommentGenerator;

    impl CodeGenerator for HashCommentGenerator {
        fn language(&self) -> &'static str {
            "hash"
        }

        fn default_naming(&self) -> NamingConfig {
            NamingConfig {
                type_name: CaseStyle::Preserve,
                enum_value: CaseStyle::Preserve,
                field: CaseStyle::Preserve,
                function: CaseStyle::Preserve,
                constant: CaseStyle::Preserve,
            }
        }

        fn line_comment_prefix(&self) -> &'static str {
            "#"
        }

        fn generate(&self, _input: &CodeExportInput<'_>) -> crate::Result<Vec<GeneratedFile>> {
            Ok(Vec::new())
        }
    }

    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let input = input_for(&meta, RustCodeGenerator.language());
    let mut writer = CodeWriter::new(input.target.options.formatting);

    HashCommentGenerator.write_file_header(&mut writer, &input);

    assert_eq!(
        writer.finish(),
        "# Copyright (c) 2026 ConfigForge contributors\n# SPDX-License-Identifier: Apache-2.0\n\n# Generated by config-forge. Do not edit.\n"
    );
}
