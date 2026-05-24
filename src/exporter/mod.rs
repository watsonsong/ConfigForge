use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    ConfigForgeError, Result,
    codegen::RuntimeDataFile,
    data::{DataResolver, DataSet, DataTable, DataValue, ResolvedValue},
    meta::{LinkedMetaModel, LinkedTableMeta, TableKind, TargetTags},
};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct TargetDataConfig {
    #[serde(default)]
    pub name: Option<String>,
    pub input: PathBuf,
    pub output: PathBuf,
    #[serde(default = "default_data_format")]
    pub format: String,
}

fn default_data_format() -> String {
    "json".to_owned()
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTable {
    meta: ExportedTableMeta,
    rows: Vec<BTreeMap<String, ExportedValue>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ExportedTablePayload {
    Collection(ExportedTable),
    Object(BTreeMap<String, ExportedValue>),
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTableMeta {
    name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ExportedValue {
    Scalar(String),
    Struct(BTreeMap<String, ExportedValue>),
}

/// Runtime data format exporter.
///
/// Implementations may be shared across worker threads. `plan_table_artifact` and
/// `serialize_table` must therefore be thread-safe and reentrant; plugins with
/// internal mutable state must synchronize it themselves or use per-task factory
/// instances in a future registry model.
pub trait DataFormatExporter: Send + Sync {
    fn name(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn extension(&self) -> &'static str;

    fn plan_table_artifact(&self, table_meta: &LinkedTableMeta<'_>) -> Result<RuntimeArtifactPlan> {
        Ok(RuntimeArtifactPlan {
            table_name: table_meta.meta.name.clone(),
            path: PathBuf::from(format!("{}.{}", table_meta.meta.name, self.extension())),
        })
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>>;

    fn matches(&self, format: &str) -> bool {
        self.name() == format || self.aliases().contains(&format)
    }
}

#[derive(Default)]
pub struct DataFormatRegistry {
    exporters: Vec<Box<dyn DataFormatExporter>>,
}

impl DataFormatRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin_exporters() -> Self {
        let mut registry = Self::new();
        registry.register(JsonDataExporter);
        registry.register(MessagePackDataExporter);
        registry
    }

    pub fn register(&mut self, exporter: impl DataFormatExporter + 'static) {
        self.exporters.push(Box::new(exporter));
    }

    pub fn get(&self, format: &str) -> Option<&dyn DataFormatExporter> {
        self.exporters
            .iter()
            .find(|exporter| exporter.matches(format))
            .map(Box::as_ref)
    }

    pub fn require(&self, format: &str) -> Result<&dyn DataFormatExporter> {
        self.get(format).ok_or_else(|| {
            ConfigForgeError::Validation(format!("no data exporter registered for `{format}`"))
        })
    }

    pub fn formats(&self) -> impl Iterator<Item = &dyn DataFormatExporter> {
        self.exporters.iter().map(Box::as_ref)
    }
}

pub fn export_runtime_data(
    out_dir: &Path,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
    exporter: &dyn DataFormatExporter,
    pool: &rayon::ThreadPool,
) -> Result<Vec<RuntimeDataFile>> {
    fs::create_dir_all(out_dir).map_err(|source| ConfigForgeError::WriteFile {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let mut plans = Vec::new();
    for table_meta in &linked_meta.tables {
        if table_meta.meta.is_exported_for(target_tags) {
            plans.push(exporter.plan_table_artifact(table_meta)?);
        }
    }
    ensure_unique_runtime_artifact_paths(&plans)?;

    // Keep at most two fully serialized artifacts in memory at once. Each artifact
    // can contain a large Vec<u8>, so this intentionally does not scale linearly
    // with --jobs until streaming/temp-file export is introduced.
    const MAX_IN_FLIGHT_RUNTIME_ARTIFACTS: usize = 2;
    let max_in_flight_artifacts = pool
        .current_num_threads()
        .clamp(1, MAX_IN_FLIGHT_RUNTIME_ARTIFACTS);
    let mut runtime_data_files = Vec::new();
    for chunk in plans.chunks(max_in_flight_artifacts) {
        let mut generated_results = pool.install(|| {
            chunk
                .par_iter()
                .enumerate()
                .map(|(index, plan)| {
                    let result: Result<GeneratedRuntimeArtifact> = (|| {
                        let table_meta = linked_meta.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "linked meta is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let table = data_set.table(&plan.table_name).ok_or_else(|| {
                            ConfigForgeError::Validation(format!(
                                "data set is missing table `{}`",
                                plan.table_name
                            ))
                        })?;
                        let content = exporter.serialize_table(table_meta, table, target_tags)?;
                        Ok(GeneratedRuntimeArtifact {
                            plan: plan.clone(),
                            content,
                        })
                    })();
                    (index, result)
                })
                .collect::<Vec<_>>()
        });
        generated_results.sort_by_key(|(index, _)| *index);

        for (_, artifact) in generated_results {
            let artifact = artifact?;
            let path = out_dir.join(&artifact.plan.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|source| ConfigForgeError::WriteFile {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
            fs::write(&path, artifact.content).map_err(|source| ConfigForgeError::WriteFile {
                path: path.clone(),
                source,
            })?;
            runtime_data_files.push(RuntimeDataFile {
                table_name: artifact.plan.table_name,
                path: artifact.plan.path.to_string_lossy().into_owned(),
            });
        }
    }
    Ok(runtime_data_files)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeArtifactPlan {
    pub table_name: String,
    pub path: PathBuf,
}

struct GeneratedRuntimeArtifact {
    plan: RuntimeArtifactPlan,
    content: Vec<u8>,
}

fn ensure_unique_runtime_artifact_paths(plans: &[RuntimeArtifactPlan]) -> Result<()> {
    let mut paths = std::collections::BTreeSet::new();
    for plan in plans {
        if !paths.insert(plan.path.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "multiple runtime data artifacts target `{}`",
                plan.path.display()
            )));
        }
    }
    Ok(())
}

pub fn build_exported_table_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<ExportedTablePayload> {
    match table_meta.meta.kind {
        TableKind::Collection => build_exported_collection_payload(table_meta, table, target_tags)
            .map(ExportedTablePayload::Collection),
        TableKind::Object => build_exported_object_payload(table_meta, table, target_tags)
            .map(ExportedTablePayload::Object),
    }
}

pub fn build_exported_collection_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<ExportedTable> {
    let visible_row_indices = table
        .rows
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|(row_index, _)| row_index)
        .collect::<Vec<_>>();
    let resolved_table = DataResolver::resolve_table_rows(
        table_meta,
        table,
        Some(target_tags),
        visible_row_indices.iter().copied(),
    )?;
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .collect::<Vec<_>>();
    let rows = visible_row_indices
        .iter()
        .enumerate()
        .map(|(resolved_row_index, row_index)| {
            let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
                ConfigForgeError::Validation(format!(
                    "resolved table `{}` is missing row {}",
                    table_meta.meta.name,
                    resolved_row_index + 1
                ))
            })?;
            let mut exported_row = BTreeMap::new();
            for field in &exported_fields {
                if let Some(value) = resolved_row.get(&field.meta.name) {
                    let value = exported_value_from_resolved(value)?;
                    exported_row.insert(field.meta.name.clone(), value);
                }
            }
            if let Some(row_source) = table.row_source(*row_index) {
                // Variant-source rows export only fragments activated by their originating source.
                for fragment_name in &row_source.fragments {
                    let Some(fragment) = table_meta.fragment(fragment_name) else {
                        continue;
                    };
                    if !fragment.meta.is_exported_for(target_tags) {
                        continue;
                    }
                    if let Some(value) = resolved_row.get(fragment.meta.name.as_str()) {
                        exported_row.insert(
                            fragment.meta.name.clone(),
                            exported_value_from_resolved(value)?,
                        );
                    }
                }
            }
            Ok(exported_row)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(ExportedTable {
        meta: ExportedTableMeta {
            name: table_meta.meta.name.clone(),
        },
        rows,
    })
}

pub fn build_exported_object_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
) -> Result<BTreeMap<String, ExportedValue>> {
    if table.rows.len() != 1 {
        return Err(ConfigForgeError::Validation(format!(
            "object table `{}` must normalize to exactly one logical row, got {}",
            table_meta.meta.name,
            table.rows.len()
        )));
    }

    let Some(row) = table.rows.first() else {
        return Ok(BTreeMap::new());
    };
    if !table_meta
        .meta
        .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
    {
        return Ok(BTreeMap::new());
    }
    let resolved_table = DataResolver::resolve_table(table_meta, table, Some(target_tags))?;
    let resolved_row = resolved_table.rows.first().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "resolved object table `{}` is missing singleton row",
            table_meta.meta.name
        ))
    })?;

    let mut exported_object = BTreeMap::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        if let Some(value) = resolved_row.get(&field.meta.name) {
            let value = exported_value_from_resolved(value)?;
            exported_object.insert(field.meta.name.clone(), value);
        }
    }

    Ok(exported_object)
}

fn exported_value_from_resolved(value: &ResolvedValue) -> Result<ExportedValue> {
    match &value.value {
        DataValue::Null => Ok(ExportedValue::Struct(BTreeMap::new())),
        DataValue::Scalar(_) => Ok(ExportedValue::Scalar(
            value.scalar_export_string().unwrap_or_default(),
        )),
        DataValue::Struct(children) | DataValue::Map(children) => children
            .iter()
            .map(|(field_name, child)| {
                Ok((field_name.clone(), exported_value_from_resolved(child)?))
            })
            .collect::<Result<BTreeMap<_, _>>>()
            .map(ExportedValue::Struct),
        DataValue::Array(_) => Err(ConfigForgeError::Validation(
            "array values are not supported by runtime data export yet".to_owned(),
        )),
    }
}

pub struct JsonDataExporter;

impl DataFormatExporter for JsonDataExporter {
    fn name(&self) -> &'static str {
        "json"
    }

    fn extension(&self) -> &'static str {
        "json"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        Ok(serde_json::to_vec_pretty(&payload)?)
    }
}

pub struct MessagePackDataExporter;

impl DataFormatExporter for MessagePackDataExporter {
    fn name(&self) -> &'static str {
        "messagepack"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["msgpack"]
    }

    fn extension(&self) -> &'static str {
        "msgpack"
    }

    fn serialize_table(
        &self,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
        target_tags: &TargetTags,
    ) -> Result<Vec<u8>> {
        let payload = build_exported_table_payload(table_meta, table, target_tags)?;
        rmp_serde::to_vec_named(&payload)
            .map_err(|source| ConfigForgeError::SerializeMessagePack { source })
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use tempfile::tempdir;

    use crate::{data::DataTable, meta::MetaModel};

    use super::*;

    struct CustomDataExporter {
        seen_tables: Mutex<Vec<String>>,
    }

    impl DataFormatExporter for CustomDataExporter {
        fn name(&self) -> &'static str {
            "custom"
        }

        fn aliases(&self) -> &'static [&'static str] {
            &["custom_alias"]
        }

        fn extension(&self) -> &'static str {
            "custom"
        }

        fn plan_table_artifact(
            &self,
            table_meta: &LinkedTableMeta<'_>,
        ) -> Result<RuntimeArtifactPlan> {
            Ok(RuntimeArtifactPlan {
                table_name: table_meta.meta.name.clone(),
                path: PathBuf::from("custom").join(format!("{}.custom", table_meta.meta.name)),
            })
        }

        fn serialize_table(
            &self,
            table_meta: &LinkedTableMeta<'_>,
            _table: &DataTable,
            _target_tags: &TargetTags,
        ) -> Result<Vec<u8>> {
            self.seen_tables
                .lock()
                .expect("seen tables lock should not be poisoned")
                .push(table_meta.meta.name.clone());
            Ok(table_meta.meta.name.as_bytes().to_vec())
        }
    }

    #[test]
    fn registry_resolves_builtin_exporters_and_aliases() {
        let registry = DataFormatRegistry::with_builtin_exporters();

        assert_eq!(registry.require("json").unwrap().extension(), "json");
        assert_eq!(
            registry.require("messagepack").unwrap().extension(),
            "msgpack"
        );
        assert_eq!(registry.require("msgpack").unwrap().extension(), "msgpack");
        assert!(registry.require("unknown").is_err());
    }

    #[test]
    fn registry_accepts_custom_exporter_plugins() {
        let mut registry = DataFormatRegistry::new();
        registry.register(CustomDataExporter {
            seen_tables: Mutex::new(Vec::new()),
        });

        assert_eq!(registry.require("custom").unwrap().extension(), "custom");
        assert_eq!(
            registry.require("custom_alias").unwrap().extension(),
            "custom"
        );
    }

    #[test]
    fn export_runtime_data_uses_registered_exporter() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = crate::data::DataSet::new(vec![DataTable::new(
            "items",
            vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
        )]);
        let out_dir = tempdir().expect("temp dir should exist");
        let exporter = CustomDataExporter {
            seen_tables: Mutex::new(Vec::new()),
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("test thread pool should build");

        let files = export_runtime_data(
            out_dir.path(),
            &linked,
            &TargetTags::empty(),
            &data_set,
            &exporter,
            &pool,
        )
        .expect("runtime data should export");

        assert_eq!(files[0].path, "custom/items.custom");
        assert_eq!(
            exporter
                .seen_tables
                .into_inner()
                .expect("seen tables lock should not be poisoned"),
            vec!["items"]
        );
        assert!(out_dir.path().join("custom/items.custom").exists());
    }

    struct ConflictDataExporter;

    impl DataFormatExporter for ConflictDataExporter {
        fn name(&self) -> &'static str {
            "conflict"
        }

        fn extension(&self) -> &'static str {
            "conflict"
        }

        fn plan_table_artifact(
            &self,
            table_meta: &LinkedTableMeta<'_>,
        ) -> Result<RuntimeArtifactPlan> {
            Ok(RuntimeArtifactPlan {
                table_name: table_meta.meta.name.clone(),
                path: PathBuf::from("same.conflict"),
            })
        }

        fn serialize_table(
            &self,
            _table_meta: &LinkedTableMeta<'_>,
            _table: &DataTable,
            _target_tags: &TargetTags,
        ) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn export_runtime_data_preserves_plan_order_for_multiple_artifacts() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "levels"
                source = "levels.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "quests"
                source = "quests.csv"
                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = crate::data::DataSet::new(vec![
            DataTable::new(
                "items",
                vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
            ),
            DataTable::new(
                "levels",
                vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
            ),
            DataTable::new(
                "quests",
                vec![BTreeMap::from([("id".to_owned(), "1".to_owned())])],
            ),
        ]);
        let out_dir = tempdir().expect("temp dir should exist");
        let exporter = CustomDataExporter {
            seen_tables: Mutex::new(Vec::new()),
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("test thread pool should build");

        let files = export_runtime_data(
            out_dir.path(),
            &linked,
            &TargetTags::empty(),
            &data_set,
            &exporter,
            &pool,
        )
        .expect("runtime data should export");

        assert_eq!(files[0].table_name, "items");
        assert_eq!(files[1].table_name, "levels");
        assert_eq!(files[2].table_name, "quests");
        assert_eq!(files[0].path, "custom/items.custom");
        assert_eq!(files[1].path, "custom/levels.custom");
        assert_eq!(files[2].path, "custom/quests.custom");
    }

    #[test]
    fn export_runtime_data_rejects_duplicate_artifact_paths_before_writing() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables]]
                name = "levels"
                source = "levels.csv"
                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let data_set = crate::data::DataSet::new(vec![
            DataTable::new("items", Vec::new()),
            DataTable::new("levels", Vec::new()),
        ]);
        let out_dir = tempdir().expect("temp dir should exist");
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("test thread pool should build");

        let error = export_runtime_data(
            out_dir.path(),
            &linked,
            &TargetTags::empty(),
            &data_set,
            &ConflictDataExporter,
            &pool,
        )
        .unwrap_err();

        assert!(
            matches!(error, ConfigForgeError::Validation(message) if message.contains("multiple runtime data artifacts target `same.conflict`"))
        );
        assert!(!out_dir.path().join("same.conflict").exists());
    }

    #[test]
    fn row_export_filter_fields_control_rows_without_runtime_output() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"
                row_export = { excluded_tags_field = "exclude_tags" }

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "exclude_tags"
                type = "string"
                export = false
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("exclude_tags".to_owned(), String::new()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "bad".to_owned()),
                    ("exclude_tags".to_owned(), "client".to_owned()),
                ]),
            ],
        );

        let payload =
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
                .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        let rows = value["rows"].as_array().expect("rows should be array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "1");
        assert!(rows[0].get("exclude_tags").is_none());
    }

    #[test]
    fn struct_json_cell_exports_as_nested_object_and_filters_children() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[structs.fields]]
                name = "count"
                type = "int"

                [[structs.fields]]
                name = "internal_note"
                type = "string"
                export = false

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                (
                    "reward".to_owned(),
                    r#"{"item_id":"coin","count":100,"internal_note":"hidden"}"#.to_owned(),
                ),
            ])],
        );

        let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
            .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["rows"][0]["id"], "1");
        assert_eq!(value["rows"][0]["reward"]["item_id"], "coin");
        assert_eq!(value["rows"][0]["reward"]["count"], "100");
        assert!(value["rows"][0]["reward"].get("internal_note").is_none());
    }

    #[test]
    fn struct_expanded_value_exports_as_nested_object_for_collection_and_object() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[structs.fields]]
                name = "max_health"
                type = "int"
                default = 100

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "id"
                type = "string"

                [[tables.fields]]
                name = "attributes"
                type = "Attributes"

                [[tables]]
                name = "game_rules"
                kind = "object"
                source = "game_rules.csv"

                [[tables.fields]]
                name = "default_attributes"
                type = "Attributes"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");

        let characters = DataTable::new(
            "characters",
            vec![BTreeMap::from([
                ("id".to_owned(), "hero".to_owned()),
                ("attributes.attack".to_owned(), "10".to_owned()),
            ])],
        );
        let payload = build_exported_table_payload(
            linked.table("characters").expect("linked table"),
            &characters,
            &TargetTags::empty(),
        )
        .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["rows"][0]["attributes"]["attack"], "10");
        assert_eq!(value["rows"][0]["attributes"]["max_health"], "100");

        let rules = DataTable::new(
            "game_rules",
            vec![BTreeMap::from([
                ("default_attributes.attack".to_owned(), "5".to_owned()),
                ("default_attributes.max_health".to_owned(), "80".to_owned()),
            ])],
        );
        let payload = build_exported_table_payload(
            linked.table("game_rules").expect("linked table"),
            &rules,
            &TargetTags::empty(),
        )
        .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert_eq!(value["default_attributes"]["attack"], "5");
        assert_eq!(value["default_attributes"]["max_health"], "80");
    }

    #[test]
    fn struct_expanded_value_exports_defaults_and_omits_empty_optional_occurrence() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[structs.fields]]
                name = "max_health"
                type = "int"
                default = 100

                [[structs]]
                name = "PlainReward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "required_attributes"
                type = "Attributes"
                required = true

                [[tables.fields]]
                name = "optional_attributes"
                type = "Attributes"

                [[tables.fields]]
                name = "optional_reward"
                type = "PlainReward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("characters").expect("linked table");
        let table = DataTable::new(
            "characters",
            vec![BTreeMap::from([
                ("optional_attributes.attack".to_owned(), String::new()),
                ("optional_attributes.max_health".to_owned(), String::new()),
                ("optional_reward.item_id".to_owned(), String::new()),
            ])],
        );

        let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
            .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["rows"][0]["required_attributes"]["max_health"], "100");
        assert_eq!(value["rows"][0]["optional_attributes"]["max_health"], "100");
        assert!(
            value["rows"][0]["optional_attributes"]
                .get("attack")
                .is_none()
        );
        assert!(value["rows"][0].get("optional_reward").is_none());
    }

    #[test]
    fn messagepack_export_preserves_expanded_struct_nested_map() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Attributes"

                [[structs.fields]]
                name = "attack"
                type = "int"

                [[tables]]
                name = "characters"
                source = "characters.csv"

                [[tables.fields]]
                name = "attributes"
                type = "Attributes"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("characters").expect("linked table");
        let table = DataTable::new(
            "characters",
            vec![BTreeMap::from([(
                "attributes.attack".to_owned(),
                "10".to_owned(),
            )])],
        );

        let bytes = MessagePackDataExporter
            .serialize_table(table_meta, &table, &TargetTags::empty())
            .expect("messagepack should serialize");
        let value: serde_json::Value =
            rmp_serde::from_slice(&bytes).expect("messagepack should decode");

        assert_eq!(value["rows"][0]["attributes"]["attack"], "10");
    }

    #[test]
    fn object_table_exports_single_object_and_filters_fields() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[structs.fields]]
                name = "count"
                type = "int"
                export_when = { any = ["server"] }

                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "max_level"
                type = "int"

                [[tables.fields]]
                name = "start_gold"
                type = "int"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("game_config").expect("linked table");
        let table = DataTable::new(
            "game_config",
            vec![BTreeMap::from([
                ("max_level".to_owned(), "100".to_owned()),
                ("start_gold".to_owned(), "500".to_owned()),
                (
                    "reward".to_owned(),
                    r#"{"item_id":"coin","count":10}"#.to_owned(),
                ),
            ])],
        );

        let payload =
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
                .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["max_level"], "100");
        assert!(value.get("start_gold").is_none());
        assert_eq!(value["reward"]["item_id"], "coin");
        assert!(value["reward"].get("count").is_none());
        assert!(value.get("rows").is_none());
        assert!(value.get("meta").is_none());
    }

    #[test]
    fn object_table_only_header_exports_empty_or_default_object() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "max_level"
                type = "int"
                default = 100
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("game_config").expect("linked table");
        let table = crate::validation::normalize_table(
            table_meta,
            &DataTable::new("game_config", vec![BTreeMap::new()]),
        )
        .expect("defaults should apply");

        let payload = build_exported_table_payload(table_meta, &table, &TargetTags::empty())
            .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["max_level"], "100");

        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "empty_config"
                kind = "object"
                source = "empty_config.csv"

                [[tables.fields]]
                name = "optional_note"
                type = "string"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("empty_config").expect("linked table");
        let payload = build_exported_table_payload(
            table_meta,
            &DataTable::new("empty_config", vec![BTreeMap::new()]),
            &TargetTags::empty(),
        )
        .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");
        assert!(
            value
                .as_object()
                .expect("payload should be object")
                .is_empty()
        );
    }

    #[test]
    fn object_table_exports_empty_object_when_all_fields_hidden_for_target() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "game_config"
                kind = "object"
                source = "game_config.csv"

                [[tables.fields]]
                name = "server_only"
                type = "string"
                export_when = { any = ["server"] }
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("game_config").expect("linked table");
        let table = DataTable::new(
            "game_config",
            vec![BTreeMap::from([(
                "server_only".to_owned(),
                "hidden".to_owned(),
            )])],
        );

        let payload =
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
                .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert!(
            value
                .as_object()
                .expect("payload should be object")
                .is_empty()
        );
    }

    #[test]
    fn fragment_export_uses_nested_objects_and_presence() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[tables]]
                name = "items"
                polymorphic = true
                sources = [
                    { provider = "csv", path = "weapons.csv", fragments = ["weapon"] },
                    { provider = "csv", path = "armors.csv", fragments = ["armor"] },
                ]

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fragments]]
                name = "weapon"

                [[tables.fragments.fields]]
                name = "power"
                type = "int"

                [[tables.fragments.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fragments]]
                name = "armor"

                [[tables.fragments.fields]]
                name = "power"
                type = "int"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new_with_sources(
            "items",
            vec![
                BTreeMap::from([
                    ("id".to_owned(), "1".to_owned()),
                    ("weapon.power".to_owned(), "10".to_owned()),
                    ("weapon.server_note".to_owned(), "hidden".to_owned()),
                ]),
                BTreeMap::from([
                    ("id".to_owned(), "2".to_owned()),
                    ("armor.power".to_owned(), "20".to_owned()),
                ]),
            ],
            vec![
                crate::data::RowSourceInfo {
                    source_index: 0,
                    source_path: PathBuf::from("weapons.csv"),
                    source_row_number: 1,
                    fragments: vec!["weapon".to_owned()],
                },
                crate::data::RowSourceInfo {
                    source_index: 1,
                    source_path: PathBuf::from("armors.csv"),
                    source_row_number: 1,
                    fragments: vec!["armor".to_owned()],
                },
            ],
        )
        .expect("table should build");

        let payload =
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
                .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert_eq!(value["rows"][0]["id"], "1");
        assert_eq!(value["rows"][0]["weapon"]["power"], "10");
        assert!(value["rows"][0]["weapon"].get("server_note").is_none());
        assert!(value["rows"][0].get("armor").is_none());
        assert_eq!(value["rows"][1]["armor"]["power"], "20");
        assert!(value["rows"][1].get("weapon").is_none());
    }

    #[test]
    fn fragment_export_locks_empty_and_hidden_fragment_presence() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "AttributeSet"

                [[structs.fields]]
                name = "server_power"
                type = "int"
                export_when = { any = ["server"] }

                [[tables]]
                name = "items"
                source = "items.csv"
                polymorphic = true

                [[tables.fields]]
                name = "id"
                type = "int"

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

                [[tables.fragments]]
                name = "typed_marker"
                type = "AttributeSet"
                export_when = { any = ["client"] }
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new_with_sources(
            "items",
            vec![BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                ("client_marker.server_note".to_owned(), "hidden".to_owned()),
                ("server_marker.value".to_owned(), "10".to_owned()),
            ])],
            vec![crate::data::RowSourceInfo {
                source_index: 0,
                source_path: PathBuf::from("items.csv"),
                source_row_number: 1,
                fragments: vec![
                    "client_marker".to_owned(),
                    "server_marker".to_owned(),
                    "typed_marker".to_owned(),
                ],
            }],
        )
        .expect("table should build");

        let payload =
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]))
                .expect("payload should build");
        let value = serde_json::to_value(payload).expect("payload should serialize");

        assert!(
            value["rows"][0]["client_marker"]
                .as_object()
                .expect("client marker should be object")
                .is_empty()
        );
        assert!(
            value["rows"][0]["typed_marker"]
                .as_object()
                .expect("typed marker should be object")
                .is_empty()
        );
        assert!(value["rows"][0].get("server_marker").is_none());
    }

    #[test]
    fn struct_json_cell_export_errors_include_row_and_field_context() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "count"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let invalid_json_table = DataTable::new(
            "items",
            vec![BTreeMap::from([(
                "reward".to_owned(),
                "not json".to_owned(),
            )])],
        );

        let error =
            build_exported_table_payload(table_meta, &invalid_json_table, &TargetTags::empty())
                .unwrap_err();
        assert!(
            matches!(&error, ConfigForgeError::Validation(message) if message.contains("table `items` row 1 field `reward` has invalid struct JSON"))
        );

        let unknown_key_table = DataTable::new(
            "items",
            vec![BTreeMap::from([(
                "reward".to_owned(),
                r#"{"count":1,"extra":true}"#.to_owned(),
            )])],
        );
        let error =
            build_exported_table_payload(table_meta, &unknown_key_table, &TargetTags::empty())
                .unwrap_err();
        assert!(
            matches!(&error, ConfigForgeError::Validation(message) if message.contains("field `reward` contains unknown struct key `extra`"))
        );
    }

    #[test]
    fn messagepack_export_preserves_struct_nested_map() {
        let meta = MetaModel::from_toml_str(
            r#"
                [[structs]]
                name = "Reward"

                [[structs.fields]]
                name = "item_id"
                type = "string"

                [[structs.fields]]
                name = "count"
                type = "int"

                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "reward"
                type = "Reward"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("items").expect("linked table");
        let table = DataTable::new(
            "items",
            vec![BTreeMap::from([(
                "reward".to_owned(),
                r#"{"item_id":"coin","count":100}"#.to_owned(),
            )])],
        );

        let bytes = MessagePackDataExporter
            .serialize_table(table_meta, &table, &TargetTags::empty())
            .expect("messagepack should serialize");
        let value: serde_json::Value =
            rmp_serde::from_slice(&bytes).expect("messagepack should decode");

        assert_eq!(value["rows"][0]["reward"]["item_id"], "coin");
        assert_eq!(value["rows"][0]["reward"]["count"], "100");
    }

    #[test]
    fn messagepack_object_table_preserves_object_shape_without_collection_envelope() {
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
                name = "pvp_enabled"
                type = "bool"
            "#,
        )
        .expect("meta should load");
        let linked = meta.link().expect("meta should link");
        let table_meta = linked.table("game_config").expect("linked table");
        let table = DataTable::new(
            "game_config",
            vec![BTreeMap::from([
                ("max_level".to_owned(), "60".to_owned()),
                ("pvp_enabled".to_owned(), "true".to_owned()),
            ])],
        );

        let bytes = MessagePackDataExporter
            .serialize_table(table_meta, &table, &TargetTags::empty())
            .expect("messagepack should serialize");
        let value: serde_json::Value =
            rmp_serde::from_slice(&bytes).expect("messagepack should decode");

        assert_eq!(value["max_level"], "60");
        assert_eq!(value["pvp_enabled"], "true");
        assert!(value.get("rows").is_none());
        assert!(value.get("meta").is_none());
    }
}
