use std::path::PathBuf;

use crate::{
    meta::{MetaModel, TargetTags},
    ConfigForgeError,
};

use super::*;

fn linked_fixture() -> MetaModel {
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

            [[tables]]
            name = "items"
            desc = "Item table."
            source = "items.csv"
            primary_index = "by_id"

            [[tables.fields]]
            name = "id"
            type = "int"
            unique = true

            [[tables.fields]]
            name = "display_name"
            type = "string"

            [[tables.fields]]
            name = "rarity"
            type = "ItemRarity"

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

fn input_for<'a>(meta: &'a MetaModel, language: CodeExportLanguage) -> CodeExportInput<'a> {
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
            language,
            out_dir: PathBuf::from("out"),
            module_name: "generated".to_owned(),
            namespace: None,
            options: CodegenOptions::for_language(language),
        },
        runtime_data_files: vec![RuntimeDataFile {
            table_name: "items".to_owned(),
            path: "items.json".to_owned(),
        }],
        generated_at: "2026-05-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn builds_code_model_with_types_tables_and_indexes() {
    let meta = linked_fixture();
    let input = input_for(&meta, CodeExportLanguage::Rust);

    let model =
        build_code_model(input.meta, &input.target_tags, &input.runtime_data_files).expect("model");

    assert_eq!(model.enums[0].name, "ItemRarity");
    assert_eq!(model.structs[0].name, "RewardInfo");
    assert_eq!(model.tables[0].name, "items");
    assert!(model.tables[0]
        .fields
        .iter()
        .any(|field| field.name == "display_name"));
    assert!(!model.tables[0]
        .fields
        .iter()
        .any(|field| field.name == "internal_note"));
    assert!(model.tables[0]
        .indexes
        .iter()
        .any(|index| index.name == "by_id" && index.unique));
    assert!(model.tables[0]
        .indexes
        .iter()
        .any(|index| index.name == "by_rarity" && !index.unique));
    assert_eq!(model.tables[0].data_file, "items.json");
}

#[test]
fn codegen_includes_fragment_structs_and_optional_row_members() {
    let meta = MetaModel::from_toml_str(
        r#"
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
    let input = input_for(&meta, CodeExportLanguage::Rust);

    let model = build_code_model(input.meta, &TargetTags::empty(), &input.runtime_data_files)
        .expect("model should build");
    assert_eq!(model.tables[0].fragments[0].name, "weapon");
    assert!(model.tables[0].fragments[0]
        .fields
        .iter()
        .any(|field| field.name == "power"));
    assert!(!model.tables[0].fragments[0]
        .fields
        .iter()
        .any(|field| field.name == "internal_note"));

    let client_model = build_code_model(input.meta, &TargetTags::new(["client"]), &[])
        .expect("client model should build");
    let client_marker = client_model.tables[0]
        .fragments
        .iter()
        .find(|fragment| fragment.name == "client_marker")
        .expect("client marker should be generated");
    assert!(client_marker.fields.is_empty());
    assert!(!client_model.tables[0]
        .fragments
        .iter()
        .any(|fragment| fragment.name == "server_marker"));

    let rust = RustCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::Rust))
        .expect("rust code");
    let rust_content = &rust[0].content;
    assert!(rust_content.contains("pub struct ItemsWeaponFragment"));
    assert!(rust_content.contains("pub weapon: Option<ItemsWeaponFragment>"));
    assert!(rust_content.contains("pub fn has_weapon(&self) -> bool"));

    let client_rust = RustCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::Rust)
        })
        .expect("client rust code");
    let client_rust_content = &client_rust[0].content;
    assert!(client_rust_content.contains("pub struct ItemsClientMarkerFragment"));
    assert!(client_rust_content.contains("pub client_marker: Option<ItemsClientMarkerFragment>"));
    assert!(client_rust_content.contains("pub fn has_client_marker(&self) -> bool"));
    assert!(!client_rust_content.contains("server_marker"));

    let typescript = TypeScriptCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::TypeScript))
        .expect("typescript code");
    let ts_content = &typescript[0].content;
    assert!(ts_content.contains("export interface ItemsWeaponFragment"));
    assert!(ts_content.contains("weapon?: ItemsWeaponFragment;"));

    let client_typescript = TypeScriptCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::TypeScript)
        })
        .expect("client typescript code");
    let client_ts_content = &client_typescript[0].content;
    assert!(client_ts_content.contains("export interface ItemsClientMarkerFragment"));
    assert!(client_ts_content.contains("clientMarker?: ItemsClientMarkerFragment;"));
    assert!(!client_ts_content.contains("serverMarker"));

    let cpp = CppCodeGenerator
        .generate(&input_for(&meta, CodeExportLanguage::Cpp))
        .expect("cpp code");
    let cpp_content = &cpp[0].content;
    assert!(cpp_content.contains("struct ItemsWeaponFragment"));
    assert!(cpp_content.contains("std::optional<ItemsWeaponFragment> weapon"));

    let client_cpp = CppCodeGenerator
        .generate(&CodeExportInput {
            target_tags: TargetTags::new(["client"]),
            ..input_for(&meta, CodeExportLanguage::Cpp)
        })
        .expect("client cpp code");
    let client_cpp_content = &client_cpp[0].content;
    assert!(client_cpp_content.contains("struct ItemsClientMarkerFragment"));
    assert!(client_cpp_content.contains("std::optional<ItemsClientMarkerFragment> client_marker"));
    assert!(!client_cpp_content.contains("server_marker"));
}

#[test]
fn code_model_defaults_data_file_to_json_when_no_runtime_file_is_available() {
    let meta = linked_fixture();
    let linked = meta.link().expect("fixture meta should link");

    let model = build_code_model(&linked, &TargetTags::empty(), &[]).expect("model");

    assert_eq!(model.tables[0].data_file, "items.json");
}

#[test]
fn code_model_includes_visible_foreign_keys() {
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
    assert!(error
        .to_string()
        .contains("target index `items.by_id` which is not exported"));
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
    assert!(error
        .to_string()
        .contains("non-exported source field `drop_item_id`"));
}

#[test]
fn rust_generator_emits_loader_table_and_index_helpers() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.language =
        LanguageCodegenOptions::Rust(RustCodegenOptions { derive_serde: true });

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub trait RuntimeDataLoader"));
    assert!(content.contains("fn load_items_rows(&self, path: &str)"));
    assert!(content.contains("pub const DATA_FILE: &'static str = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("load_bytes"));
    assert!(content.contains("serde::Serialize, serde::Deserialize"));
    assert!(content.contains("#[serde(rename = \"common\")]"));
    assert!(content.contains("#[serde(rename = \"display_name\")]"));
    assert!(content.contains("pub enum ItemRarity"));
    assert!(content.contains("pub struct RewardInfo"));
    assert!(content.contains("pub struct ItemsRow"));
    assert!(content.contains("pub fn get_by_id"));
    assert!(content.contains("pub fn find_by_rarity"));
    assert!(content.contains("pub struct GameData"));
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
    assert!(!items
        .indexes
        .iter()
        .any(|index| index.name == "by_drop_weight"));
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

#[test]
fn generators_reject_duplicate_generated_field_names() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "foo_bar"
            type = "int"

            [[tables.fields]]
            name = "foo-bar"
            type = "int"
        "#,
    )
    .expect("meta should load");

    assert_duplicate_generated_identifier(
        RustCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::Rust)),
        "rust codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
    assert_duplicate_generated_identifier(
        CppCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::Cpp)),
        "cpp codegen",
        "duplicate generated identifier `foo_bar`",
        "table `items` row fields",
    );
    assert_duplicate_generated_identifier(
        TypeScriptCodeGenerator.generate(&input_for(&meta, CodeExportLanguage::TypeScript)),
        "typescript codegen",
        "duplicate generated identifier `fooBar`",
        "table `items` row fields",
    );
}

#[test]
fn generators_reject_duplicate_top_level_generated_types() {
    let meta = MetaModel::from_toml_str(
        r#"
            [[structs]]
            name = "items_row"

            [[structs.fields]]
            name = "value"
            type = "int"

            [[tables]]
            name = "items"
            source = "items.csv"

            [[tables.fields]]
            name = "id"
            type = "int"
        "#,
    )
    .expect("meta should load");

    for language in [
        CodeExportLanguage::Rust,
        CodeExportLanguage::Cpp,
        CodeExportLanguage::TypeScript,
    ] {
        let result = match language {
            CodeExportLanguage::Rust => RustCodeGenerator.generate(&input_for(&meta, language)),
            CodeExportLanguage::Cpp => CppCodeGenerator.generate(&input_for(&meta, language)),
            CodeExportLanguage::TypeScript => {
                TypeScriptCodeGenerator.generate(&input_for(&meta, language))
            }
        };
        assert_duplicate_generated_identifier(
            result,
            language.as_str(),
            "duplicate generated identifier `ItemsRow`",
            if language == CodeExportLanguage::TypeScript {
                "top-level exports"
            } else {
                "top-level types"
            },
        );
    }
}

fn assert_duplicate_generated_identifier(
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
fn typescript_generator_emits_async_loader_and_maps() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::TypeScript);
    input.target.options.language = LanguageCodegenOptions::TypeScript(TypeScriptCodegenOptions {
        readonly_fields: true,
    });

    let files = TypeScriptCodeGenerator
        .generate(&input)
        .expect("typescript code");
    let content = &files[0].content;

    assert!(content.contains("export interface RuntimeDataLoader"));
    assert!(content.contains("loadItemsRows(path: string): Promise<ItemsRow[]> | ItemsRow[];"));
    assert!(content.contains("static readonly dataFile = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("RuntimeDataDecoder"));
    assert!(!content.contains("loadBytes"));
    assert!(content.contains("export type ItemRarity"));
    assert!(content.contains("export interface ItemsRow"));
    assert!(content.contains("readonly displayName: string;"));
    assert!(content.contains("static async load"));
    assert!(content.contains("getById(id: number): ItemsRow | undefined"));
    assert!(content.contains("findByRarity(rarity: ItemRarity): readonly ItemsRow[]"));
    assert!(content.contains("Duplicate unique index key for ItemsTable.by_id"));
}

#[test]
fn cpp_generator_emits_loader_and_repository_declarations() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::Cpp);
    input.target.options.language = LanguageCodegenOptions::Cpp(CppCodegenOptions {
        namespace: Some("game::config".to_owned()),
    });

    let files = CppCodeGenerator.generate(&input).expect("cpp code");
    let content = &files[0].content;

    assert!(content.contains("class IRuntimeDataLoader"));
    assert!(content.contains("LoadItemsRows(const std::string& path)"));
    assert!(content.contains("static constexpr const char* DataFile = \"items.json\";"));
    assert!(!content.contains("RuntimeDataFormat"));
    assert!(!content.contains("IRuntimeDataDecoder"));
    assert!(!content.contains("IDataFileLoader"));
    assert!(content.contains("namespace game::config"));
    assert!(content.contains("} // namespace game::config"));
    assert!(content.contains("enum class ItemRarity"));
    assert!(content.contains("struct ItemsRow"));
    assert!(content.contains("class ItemsTable"));
    assert!(content.contains("const ItemsRow* GetById"));
    assert!(content.contains("std::vector<const ItemsRow*> FindByRarity"));
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
    assert_eq!(crate::codegen::naming::camel_case("item-name"), "itemName");
}

#[test]
fn parses_codegen_task_config_and_merges_naming_defaults() {
    let config: ProjectCodegenConfig = toml::from_str(
        r#"
            [[targets]]
            name = "client"

            [[targets.code]]
            language = "typescript"
            output = "Generated/TS"
            file_header = "// {{project.name}} {{datetime}}"
            keyword_conflict = "error"

            [targets.code.naming]
            field = "snake_case"

            [targets.code.typescript]
            readonly_fields = true
        "#,
    )
    .expect("config should parse");

    let target = CodeExportTarget::from_task(config.targets[0].code[0].clone());

    assert_eq!(target.language, CodeExportLanguage::TypeScript);
    assert_eq!(target.out_dir, PathBuf::from("Generated/TS"));
    assert_eq!(
        target.options.keyword_conflict,
        KeywordConflictPolicy::Error
    );
    assert_eq!(target.options.naming.type_name, CaseStyle::PascalCase);
    assert_eq!(target.options.naming.field, CaseStyle::SnakeCase);
    assert_eq!(target.options.naming.function, CaseStyle::CamelCase);
    assert_eq!(
        target.options.language,
        LanguageCodegenOptions::TypeScript(TypeScriptCodegenOptions {
            readonly_fields: true,
        })
    );
}

#[test]
fn render_file_header_replaces_project_variables_and_datetime() {
    let meta = MetaModel::from_toml_str("").expect("meta should parse");
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.file_header = Some(
        "// {{project.name}} | {{project.desc}} | {{project.copyright}} | {{datetime}}".to_owned(),
    );

    assert_eq!(
        render_file_header(&input),
        "// demo | Demo config | Copyright Demo | 2026-05-17T00:00:00Z\n"
    );
}
