use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use rayon::prelude::*;
use serde::Serialize;

use crate::{
    codegen::RuntimeDataFile,
    data::{DataRow, DataSet, DataTable},
    meta::{LinkedMetaModel, LinkedTableMeta, TargetTags},
    ConfigForgeError, Result,
};

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTable {
    meta: ExportedTableMeta,
    rows: Vec<DataRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExportedTableMeta {
    name: String,
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
) -> ExportedTable {
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
        .map(|field| field.meta.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let rows = table
        .rows
        .iter()
        .filter(|row| {
            table_meta
                .meta
                .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
        })
        .map(|row| {
            row.iter()
                .filter(|(field, _)| exported_fields.contains(field.as_str()))
                .map(|(field, value)| (field.clone(), value.clone()))
                .collect::<BTreeMap<_, _>>()
        })
        .collect();

    ExportedTable {
        meta: ExportedTableMeta {
            name: table_meta.meta.name.clone(),
        },
        rows,
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
        let payload = build_exported_table_payload(table_meta, table, target_tags);
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
        let payload = build_exported_table_payload(table_meta, table, target_tags);
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
            build_exported_table_payload(table_meta, &table, &TargetTags::new(["client"]));
        let value = serde_json::to_value(payload).expect("payload should serialize");

        let rows = value["rows"].as_array().expect("rows should be array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], "1");
        assert!(rows[0].get("exclude_tags").is_none());
    }
}
