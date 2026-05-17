use std::path::PathBuf;

use crate::meta::MetaModel;

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
            tool_only = true

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
        runtime_data_format: RuntimeDataFormat::Json,
        generated_at: "2026-05-17T00:00:00Z".to_owned(),
    }
}

#[test]
fn builds_code_model_with_types_tables_and_indexes() {
    let meta = linked_fixture();
    let input = input_for(&meta, CodeExportLanguage::Rust);

    let model = build_code_model(input.meta, &input.runtime_data_files).expect("model");

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
}

#[test]
fn rust_generator_emits_loader_table_and_index_helpers() {
    let meta = linked_fixture();
    let mut input = input_for(&meta, CodeExportLanguage::Rust);
    input.target.options.language =
        LanguageCodegenOptions::Rust(RustCodegenOptions { derive_serde: true });

    let files = RustCodeGenerator.generate(&input).expect("rust code");
    let content = &files[0].content;

    assert!(content.contains("pub trait DataFileLoader"));
    assert!(content.contains("serde::Serialize, serde::Deserialize"));
    assert!(content.contains("pub enum ItemRarity"));
    assert!(content.contains("pub struct RewardInfo"));
    assert!(content.contains("pub struct ItemsRow"));
    assert!(content.contains("pub fn get_by_id"));
    assert!(content.contains("pub fn find_by_rarity"));
    assert!(content.contains("pub struct GameData"));
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

    assert!(content.contains("export interface DataFileLoader"));
    assert!(content.contains("export type ItemRarity"));
    assert!(content.contains("export interface ItemsRow"));
    assert!(content.contains("readonly displayName: string;"));
    assert!(content.contains("static async load"));
    assert!(content.contains("getById(id: number): ItemsRow | undefined"));
    assert!(content.contains("findByRarity(rarity: ItemRarity): readonly ItemsRow[]"));
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

    assert!(content.contains("class IDataFileLoader"));
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
