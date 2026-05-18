use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use rayon::prelude::*;
use serde::Serialize;

use crate::{
    codegen::RuntimeDataFile,
    data::{DataSet, DataTable},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, LinkedTableMeta,
        TargetTags,
    },
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTable {
    meta: ExportedTableMeta,
    rows: Vec<BTreeMap<String, ExportedValue>>,
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
) -> Result<ExportedTable> {
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .collect::<Vec<_>>();
    let rows = table
        .rows
        .iter()
        .enumerate()
        .filter(|row| {
            let row = row.1;
            table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|(row_index, row)| {
            let mut exported_row = BTreeMap::new();
            for field in &exported_fields {
                let Some(raw) = row.get(&field.meta.name) else {
                    continue;
                };
                if let Some(value) = export_field_value(
                    table_meta.meta.name.as_str(),
                    row_index + 1,
                    field.meta.name.as_str(),
                    field,
                    raw,
                    target_tags,
                )? {
                    exported_row.insert(field.meta.name.clone(), value);
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

fn export_field_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    raw: &str,
    target_tags: &TargetTags,
) -> Result<Option<ExportedValue>> {
    match &field.linked_field {
        LinkedField::Struct(struct_type) => {
            if raw.trim().is_empty() {
                return Ok(None);
            }
            let value: serde_json::Value = serde_json::from_str(raw).map_err(|source| {
                ConfigForgeError::Validation(format!(
                    "table `{}` row {} field `{}` has invalid struct JSON: {}",
                    table_name, row_number, field_path, source
                ))
            })?;
            export_struct_value(
                table_name,
                row_number,
                field_path,
                struct_type,
                &value,
                target_tags,
            )
            .map(Some)
        }
        _ => Ok(Some(ExportedValue::Scalar(raw.to_owned()))),
    }
}

fn export_struct_value(
    table_name: &str,
    row_number: usize,
    field_path: &str,
    struct_type: &LinkedStructType<'_>,
    value: &serde_json::Value,
    target_tags: &TargetTags,
) -> Result<ExportedValue> {
    let object = value.as_object().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "table `{}` row {} field `{}` must be a JSON object for struct `{}`",
            table_name, row_number, field_path, struct_type.meta.name
        ))
    })?;
    for key in object.keys() {
        if !struct_type
            .fields
            .iter()
            .any(|field| field.meta.name == key.as_str())
        {
            return Err(ConfigForgeError::Validation(format!(
                "table `{}` row {} field `{}` contains unknown struct key `{}`",
                table_name, row_number, field_path, key
            )));
        }
    }
    let mut exported = BTreeMap::new();
    for field in struct_type
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        let default_value = field.meta.default.as_ref().map(toml_default_to_json_value);
        let Some(value) = object.get(&field.meta.name).or(default_value.as_ref()) else {
            continue;
        };
        if json_value_is_empty(value) {
            continue;
        }
        let child_path = format!("{field_path}.{}", field.meta.name);
        let exported_value = match &field.linked_field {
            LinkedField::Struct(nested_struct) => export_struct_value(
                table_name,
                row_number,
                &child_path,
                nested_struct,
                value,
                target_tags,
            )?,
            _ => ExportedValue::Scalar(json_value_to_scalar_cell(value)?),
        };
        exported.insert(field.meta.name.clone(), exported_value);
    }
    Ok(ExportedValue::Struct(exported))
}

fn toml_default_to_json_value(value: &toml::Value) -> serde_json::Value {
    match value {
        toml::Value::String(value) => serde_json::Value::String(value.clone()),
        toml::Value::Integer(value) => serde_json::Value::Number((*value).into()),
        toml::Value::Float(value) => serde_json::Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        toml::Value::Boolean(value) => serde_json::Value::Bool(*value),
        _ => serde_json::Value::Null,
    }
}

fn json_value_is_empty(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Null)
        || value.as_str().map(str::trim).is_some_and(str::is_empty)
}

fn json_value_to_scalar_cell(value: &serde_json::Value) -> Result<String> {
    match value {
        serde_json::Value::String(value) => Ok(value.clone()),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        serde_json::Value::Bool(value) => Ok(value.to_string()),
        _ => Err(ConfigForgeError::Validation(
            "struct scalar field value must be string, number, or bool".to_owned(),
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
                    ("id".to_owned(), "2".to_owned()),
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
}
