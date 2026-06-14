use std::{fs, path::PathBuf};

use crate::{
    ConfigForgeError, Diagnostic,
    diagnostic::code::CF_DATA_INVALID_ASSET_REF,
    meta::{MetaModel, TargetTags},
    project::{AssetSearchPathConfig, AssetsConfig},
};

use super::*;

fn linked_meta(source: &str) -> MetaModel {
    toml::from_str::<MetaModel>(source)
        .expect("meta should parse")
        .link()
        .expect("meta should link")
        .model
        .clone()
}

fn resolver(meta: &MetaModel, assets: AssetsConfig) -> AssetResolver {
    let linked = meta.link().expect("meta should link");
    AssetResolver::new(&linked, &assets).expect("resolver should build")
}

fn search_path(name: &str, root: PathBuf, mount: Option<&str>) -> AssetSearchPathConfig {
    AssetSearchPathConfig {
        name: name.to_owned(),
        root,
        kind: "filesystem".to_owned(),
        mount: mount.map(str::to_owned),
    }
}

fn expect_single_diagnostic(error: ConfigForgeError) -> Diagnostic {
    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured diagnostic report");
    };
    assert_eq!(report.diagnostics.len(), 1);
    report.diagnostics.into_iter().next().unwrap()
}

fn expect_asset_diagnostic(error: ConfigForgeError) -> Diagnostic {
    let diagnostic = expect_single_diagnostic(error);
    assert_eq!(diagnostic.code, CF_DATA_INVALID_ASSET_REF);
    assert_eq!(diagnostic.source.as_deref(), Some("asset.resolver"));
    diagnostic
}

#[test]
fn resolves_relative_path_with_required_extension() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("UI/Icon_Sword.png");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                base_dirs = ["UI"]
                extensions = [".png"]
                path_style = "relative"
                extension_policy = "required"
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    let resolved = resolver
        .resolve("ui_icon", "Icon_Sword.png")
        .expect("asset should resolve");

    assert_eq!(resolved.canonical_export_ref, "UI/Icon_Sword.png");
    assert_eq!(resolved.search_path_id, "game");
    assert_eq!(
        resolved.search_path_relative_path,
        PathBuf::from("UI/Icon_Sword.png")
    );
    assert_eq!(resolved.resolved_filesystem_path, asset_path);
    assert!(!resolved.completion_applied);
}

#[test]
fn resolves_unreal_mounted_object_path() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path(
                "game",
                temp_dir.path().to_path_buf(),
                Some("/Game"),
            )],
            ..AssetsConfig::default()
        },
    );

    let resolved = resolver
        .resolve("unreal_widget_class", "/Game/UI/WBP_Item.WBP_Item_C")
        .expect("asset should resolve");

    assert_eq!(
        resolved.canonical_export_ref,
        "/Game/UI/WBP_Item.WBP_Item_C"
    );
    assert_eq!(
        resolved.search_path_relative_path,
        PathBuf::from("UI/WBP_Item.uasset")
    );
    assert_eq!(resolved.object_name.as_deref(), Some("WBP_Item_C"));
    assert_eq!(resolved.resolved_filesystem_path, asset_path);
}

#[test]
fn strips_unreal_wrappers_before_resolving() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                strip_wrappers = ["BlueprintGeneratedClass'{value}'"]
                object_name = "{stem}_C"
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path(
                "game",
                temp_dir.path().to_path_buf(),
                Some("/Game"),
            )],
            ..AssetsConfig::default()
        },
    );

    let resolved = resolver
        .resolve(
            "unreal_widget_class",
            "BlueprintGeneratedClass'/Game/UI/WBP_Item.WBP_Item_C'",
        )
        .expect("asset should resolve");

    assert_eq!(
        resolved.canonical_export_ref,
        "/Game/UI/WBP_Item.WBP_Item_C"
    );
    assert!(resolved.completion_applied);
}

#[test]
fn completes_unique_unreal_asset_candidate() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "unreal_asset"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
                allow_auto_complete = true

                [[asset_kinds]]
                name = "ui_widget"
                extends = "unreal_asset"
                base_dirs = ["UI"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path(
                "game",
                temp_dir.path().to_path_buf(),
                Some("/Game"),
            )],
            ..AssetsConfig::default()
        },
    );

    let resolved = resolver
        .resolve("ui_widget", "WBP_Item")
        .expect("asset should resolve");

    assert_eq!(
        resolved.canonical_export_ref,
        "/Game/UI/WBP_Item.WBP_Item_C"
    );
    assert_eq!(
        resolved.search_path_relative_path,
        PathBuf::from("UI/WBP_Item.uasset")
    );
    assert!(resolved.completion_applied);
    assert_eq!(resolved.object_name.as_deref(), Some("WBP_Item_C"));
}

#[test]
fn reports_ambiguous_completion_candidates() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    for dir in ["UI", "DevUI"] {
        let asset_path = temp_dir.path().join(dir).join("WBP_Item.uasset");
        fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
        fs::write(&asset_path, []).expect("write asset");
    }
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_widget"
                search_paths = ["game"]
                base_dirs = ["UI", "DevUI"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
                allow_auto_complete = true
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path(
                "game",
                temp_dir.path().to_path_buf(),
                Some("/Game"),
            )],
            ..AssetsConfig::default()
        },
    );

    let error = resolver.resolve("ui_widget", "WBP_Item").unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("ambiguously"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("WBP_Item"));
}

#[test]
fn rejects_absolute_and_parent_escape_paths() {
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", PathBuf::from("/tmp/assets"), None)],
            ..AssetsConfig::default()
        },
    );

    let absolute_error = resolver.resolve("ui_icon", "/tmp/Icon.png").unwrap_err();
    let diagnostic = expect_asset_diagnostic(absolute_error);
    assert!(
        diagnostic.message.contains("must be relative") || diagnostic.message.contains("escape")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("/tmp/Icon.png"));

    let parent_error = resolver.resolve("ui_icon", "../Icon.png").unwrap_err();
    let diagnostic = expect_asset_diagnostic(parent_error);
    assert!(diagnostic.message.contains("must not escape"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("../Icon.png"));
}

#[test]
fn require_existing_reports_missing_file() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_existing = true
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    let error = resolver.resolve("ui_icon", "Missing.png").unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("missing file"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("Missing.png"));
}

#[test]
fn require_sidecars_reports_missing_pair_file() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("Icon_Sword.png");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                require_sidecars = ["{path}.meta"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    let error = resolver.resolve("ui_icon", "Icon_Sword.png").unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("missing sidecar"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("Icon_Sword.png"));
}

#[test]
fn validates_name_prefixes_and_suffixes() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    fs::write(temp_dir.path().join("BadName.png"), []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                name_prefixes = ["Icon_"]
                name_suffixes = ["_UI"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    let error = resolver.resolve("ui_icon", "BadName.png").unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("required prefixes"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("BadName"));
}

#[test]
fn validates_object_name_template() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("UI/WBP_Item.uasset");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "unreal_widget_class"
                search_paths = ["game"]
                extensions = [".uasset"]
                path_style = "mounted_object"
                object_name = "{stem}_C"
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path(
                "game",
                temp_dir.path().to_path_buf(),
                Some("/Game"),
            )],
            ..AssetsConfig::default()
        },
    );

    let error = resolver
        .resolve("unreal_widget_class", "/Game/UI/WBP_Item.WBP_Item")
        .unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("object name"));
    assert!(diagnostic.message.contains("WBP_Item_C"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("WBP_Item"));
}

#[test]
fn validates_target_gated_deny_prefixes() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let asset_path = temp_dir.path().join("Dev/Icon_Debug.png");
    fs::create_dir_all(asset_path.parent().expect("parent")).expect("create dirs");
    fs::write(&asset_path, []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                deny_path_prefixes = ["Dev/"]
                deny_when_tags = ["shipping"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    resolver
        .resolve_for_target("ui_icon", "Dev/Icon_Debug.png", &TargetTags::new(["dev"]))
        .expect("dev target should allow dev asset");
    let error = resolver
        .resolve_for_target(
            "ui_icon",
            "Dev/Icon_Debug.png",
            &TargetTags::new(["shipping"]),
        )
        .unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("denied by prefix"));
    assert_eq!(
        diagnostic.current_value.as_deref(),
        Some("Dev/Icon_Debug.png")
    );
}

#[test]
fn validates_allow_and_deny_patterns() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    for file_name in ["Icon_Sword.png", "Tmp_Icon.png"] {
        fs::write(temp_dir.path().join(file_name), []).expect("write asset");
    }
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]
                allow_patterns = ["^Icon_"]
                deny_patterns = ["Tmp"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    resolver
        .resolve("ui_icon", "Icon_Sword.png")
        .expect("allowed pattern should pass");
    let error = resolver.resolve("ui_icon", "Tmp_Icon.png").unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(
        diagnostic
            .message
            .contains("does not match any allow pattern")
            || diagnostic.message.contains("denied by pattern")
    );
    assert_eq!(diagnostic.current_value.as_deref(), Some("Tmp_Icon.png"));
}

#[test]
fn validates_regex_rules_for_specific_targets() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    fs::write(temp_dir.path().join("Icon_Sword.png"), []).expect("write asset");
    let meta = linked_meta(
        r#"
                [[asset_kinds]]
                name = "ui_icon"
                search_paths = ["game"]
                extensions = [".png"]

                [[asset_kinds.regex_rules]]
                name = "shipping_icon_stem"
                target = "stem"
                pattern = "^Ship_"
                message = "shipping icon stem must start with Ship_"
                when_tags = ["shipping"]
            "#,
    );
    let resolver = resolver(
        &meta,
        AssetsConfig {
            search_paths: vec![search_path("game", temp_dir.path().to_path_buf(), None)],
            ..AssetsConfig::default()
        },
    );

    resolver
        .resolve_for_target("ui_icon", "Icon_Sword.png", &TargetTags::new(["client"]))
        .expect("non-shipping target should skip regex rule");
    let error = resolver
        .resolve_for_target("ui_icon", "Icon_Sword.png", &TargetTags::new(["shipping"]))
        .unwrap_err();

    let diagnostic = expect_asset_diagnostic(error);
    assert!(diagnostic.message.contains("shipping icon stem"));
    assert_eq!(diagnostic.current_value.as_deref(), Some("Icon_Sword"));
}
