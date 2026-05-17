use std::{collections::BTreeMap, fs, path::Path};

use serde::Serialize;

use crate::{
    codegen::RuntimeDataFile,
    data::{DataRow, DataSet, DataTable},
    meta::{LinkedMetaModel, LinkedTableMeta},
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

pub trait DataFormatExporter {
    fn name(&self) -> &'static str;

    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    fn extension(&self) -> &'static str;

    fn export_table(
        &self,
        out_dir: &Path,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
    ) -> Result<String>;

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
    data_set: &DataSet,
    exporter: &dyn DataFormatExporter,
) -> Result<Vec<RuntimeDataFile>> {
    fs::create_dir_all(out_dir).map_err(|source| ConfigForgeError::WriteFile {
        path: out_dir.to_path_buf(),
        source,
    })?;

    let mut runtime_data_files = Vec::new();
    for table_meta in &linked_meta.tables {
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        let path = exporter.export_table(out_dir, table_meta, table)?;
        runtime_data_files.push(RuntimeDataFile {
            table_name: table_meta.meta.name.clone(),
            path,
        });
    }
    Ok(runtime_data_files)
}

pub fn build_exported_table_payload(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
) -> ExportedTable {
    let exported_fields = table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_runtime_exported())
        .map(|field| field.meta.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let rows = table
        .rows
        .iter()
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

    fn export_table(
        &self,
        out_dir: &Path,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
    ) -> Result<String> {
        let payload = build_exported_table_payload(table_meta, table);
        let content = serde_json::to_string_pretty(&payload)?;
        let file_name = format!("{}.{}", table_meta.meta.name, self.extension());
        let path = out_dir.join(&file_name);
        fs::write(&path, content).map_err(|source| ConfigForgeError::WriteFile { path, source })?;
        Ok(file_name)
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

    fn export_table(
        &self,
        out_dir: &Path,
        table_meta: &LinkedTableMeta<'_>,
        table: &DataTable,
    ) -> Result<String> {
        let payload = build_exported_table_payload(table_meta, table);
        let content = rmp_serde::to_vec_named(&payload)
            .map_err(|source| ConfigForgeError::SerializeMessagePack { source })?;
        let file_name = format!("{}.{}", table_meta.meta.name, self.extension());
        let path = out_dir.join(&file_name);
        fs::write(&path, content).map_err(|source| ConfigForgeError::WriteFile { path, source })?;
        Ok(file_name)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, collections::BTreeMap, fs, path::Path};

    use tempfile::tempdir;

    use crate::{data::DataTable, meta::MetaModel};

    use super::*;

    struct CustomDataExporter {
        seen_tables: RefCell<Vec<String>>,
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

        fn export_table(
            &self,
            out_dir: &Path,
            table_meta: &LinkedTableMeta<'_>,
            _table: &DataTable,
        ) -> Result<String> {
            self.seen_tables
                .borrow_mut()
                .push(table_meta.meta.name.clone());
            let file_name = format!("{}.{}", table_meta.meta.name, self.extension());
            let path = out_dir.join(&file_name);
            fs::write(&path, table_meta.meta.name.as_bytes())
                .map_err(|source| ConfigForgeError::WriteFile { path, source })?;
            Ok(file_name)
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
            seen_tables: RefCell::new(Vec::new()),
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
            seen_tables: RefCell::new(Vec::new()),
        };

        let files = export_runtime_data(out_dir.path(), &linked, &data_set, &exporter)
            .expect("runtime data should export");

        assert_eq!(files[0].path, "items.custom");
        assert_eq!(exporter.seen_tables.into_inner(), vec!["items"]);
        assert!(out_dir.path().join("items.custom").exists());
    }
}
