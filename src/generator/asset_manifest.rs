// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    ConfigForgeError, Result,
    artifact::{Artifact, ArtifactSink},
    data::{DataRow, DataSet, DataTable},
    data_artifact::{AggregateDataExporter, TargetDataConfig},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedMetaModel, LinkedStructType, LinkedTableMeta,
        PrimitiveFieldType, TableKind, TargetTags,
    },
};

use super::{ArtifactGenerator, GeneratorCommonInput, GeneratorDataInput, GeneratorRequirements};

pub const FORMAT: &str = "asset_manifest";

pub struct AssetManifestExporter;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct AssetManifestOptions {
    file_name: Option<String>,
    format: AssetManifestFormat,
    csv_mode: AssetManifestCsvMode,
    deduplicate: bool,
    include_referenced_by: bool,
    include_source_location: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AssetManifestFormat {
    Json,
    Csv,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum AssetManifestCsvMode {
    UnrealDatatable,
    Simple,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AssetManifestEntry {
    kind: String,
    path: String,
    referenced_by: Vec<AssetReferenceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct AssetReferenceEntry {
    table: String,
    row: String,
    field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonAssetManifest<'a> {
    version: u32,
    target_tags: Vec<String>,
    assets: &'a [JsonAssetEntry],
}

#[derive(Debug, Serialize)]
struct JsonAssetEntry {
    kind: String,
    path: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    referenced_by: Vec<AssetReferenceEntry>,
}

impl Default for AssetManifestOptions {
    fn default() -> Self {
        Self {
            file_name: None,
            format: AssetManifestFormat::Json,
            csv_mode: AssetManifestCsvMode::UnrealDatatable,
            deduplicate: true,
            include_referenced_by: true,
            include_source_location: false,
        }
    }
}

impl AggregateDataExporter for AssetManifestExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["assets"]
    }

    fn plan_artifacts(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        _linked_meta: &LinkedMetaModel<'_>,
        _target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>> {
        let options = options_from_task(task)?;
        Ok(vec![out_dir.join(options.file_name())])
    }

    fn export(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>> {
        let (file_name, content) = render_asset_manifest(task, linked_meta, target_tags, data_set)?;
        Ok(vec![Artifact::text(out_dir.join(file_name), content)])
    }

    fn export_to_sink(
        &self,
        out_dir: &Path,
        task: &TargetDataConfig,
        linked_meta: &LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let (file_name, content) = render_asset_manifest(task, linked_meta, target_tags, data_set)?;
        let mut stream = sink.create(out_dir.join(file_name))?;
        stream.write_all_artifact(content.as_bytes())?;
        stream.finish()?;
        Ok(())
    }
}

impl ArtifactGenerator for AssetManifestExporter {
    fn type_id(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["assets"]
    }

    fn requirements(&self) -> GeneratorRequirements {
        GeneratorRequirements::meta_and_data().with_assets()
    }

    fn generate_to_sink(
        &self,
        _input: &GeneratorCommonInput<'_>,
        _sink: &ArtifactSink,
    ) -> Result<()> {
        Err(ConfigForgeError::Validation(format!(
            "generator `{}` requires Data-backed artifact input",
            FORMAT
        )))
    }

    fn plan_artifacts(&self, input: &GeneratorCommonInput<'_>) -> Result<Vec<PathBuf>> {
        let task = target_data_config_from_common(input);
        <Self as AggregateDataExporter>::plan_artifacts(
            self,
            &input.output,
            &task,
            input.meta,
            &input.target_tags,
        )
    }

    fn generate_with_data_to_sink(
        &self,
        input: &GeneratorDataInput<'_, '_>,
        sink: &ArtifactSink,
    ) -> Result<()> {
        let task = target_data_config_from_common(&input.common);
        <Self as AggregateDataExporter>::export_to_sink(
            self,
            &input.common.output,
            &task,
            input.common.meta,
            &input.common.target_tags,
            input.data_set,
            sink,
        )
    }
}

fn target_data_config_from_common(input: &GeneratorCommonInput<'_>) -> TargetDataConfig {
    TargetDataConfig {
        name: Some(input.task_name.clone()),
        format: input.type_id.clone(),
        output: input.output.clone(),
        options: input.options.clone(),
    }
}

fn render_asset_manifest(
    task: &TargetDataConfig,
    linked_meta: &LinkedMetaModel<'_>,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<(String, String)> {
    let options = options_from_task(task)?;
    let mut references = Vec::new();
    for table_meta in &linked_meta.tables {
        if !table_meta.meta.is_exported_for(target_tags) {
            continue;
        }
        let table = data_set.table(&table_meta.meta.name).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "data set is missing table `{}`",
                table_meta.meta.name
            ))
        })?;
        extract_table_asset_references(&mut references, table_meta, table, target_tags, &options);
    }

    let entries = build_manifest_entries(references, &options);
    let content = match options.format {
        AssetManifestFormat::Json => render_json_manifest(&entries, target_tags, &options)?,
        AssetManifestFormat::Csv => render_csv_manifest(&entries, &options)?,
    };
    Ok((options.file_name(), content))
}

impl AssetManifestOptions {
    fn file_name(&self) -> String {
        self.file_name.clone().unwrap_or_else(|| match self.format {
            AssetManifestFormat::Json => "asset_manifest.json".to_owned(),
            AssetManifestFormat::Csv => "asset_manifest.csv".to_owned(),
        })
    }
}

fn options_from_task(task: &TargetDataConfig) -> Result<AssetManifestOptions> {
    if let Some(value) = task.options.get("asset_manifest") {
        return value.clone().try_into().map_err(|error| {
            ConfigForgeError::Validation(format!("invalid asset_manifest options: {error}"))
        });
    }
    if task.options.is_empty() {
        return Ok(AssetManifestOptions::default());
    }
    toml::Value::Table(task.options.clone().into_iter().collect())
        .try_into()
        .map_err(|error| {
            ConfigForgeError::Validation(format!("invalid asset_manifest options: {error}"))
        })
}

fn extract_table_asset_references(
    references: &mut Vec<AssetManifestEntry>,
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
    options: &AssetManifestOptions,
) {
    match table_meta.meta.kind {
        TableKind::Collection => {
            for (row_index, row) in table.rows.iter().enumerate() {
                if !table_meta
                    .meta
                    .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
                {
                    continue;
                }
                extract_row_asset_references(
                    references,
                    table_meta,
                    table,
                    row_index,
                    row,
                    target_tags,
                    options,
                );
            }
        }
        TableKind::Object => {
            if let Some(row) = table.rows.first() {
                if table_meta
                    .meta
                    .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
                {
                    extract_row_asset_references(
                        references,
                        table_meta,
                        table,
                        0,
                        row,
                        target_tags,
                        options,
                    );
                }
            }
        }
    }
}

fn extract_row_asset_references(
    references: &mut Vec<AssetManifestEntry>,
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    row_index: usize,
    row: &DataRow,
    target_tags: &TargetTags,
    options: &AssetManifestOptions,
) {
    let record_key = record_key(table_meta, table, row_index, row);
    let source = source_location(table, row_index).filter(|_| options.include_source_location);
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        extract_asset_field_path(
            references,
            table_meta.meta.name.as_str(),
            row,
            field.meta.name.as_str(),
            field,
            &record_key,
            source.as_deref(),
        );
    }

    let active_fragments = table
        .row_source(row_index)
        .map(|source| source.fragments.as_slice())
        .unwrap_or(&[]);
    for fragment_name in active_fragments {
        let Some(fragment) = table_meta.fragment(fragment_name) else {
            continue;
        };
        if !fragment.meta.is_exported_for(target_tags) {
            continue;
        }
        if let Some(struct_type) = fragment.struct_type() {
            extract_struct_asset_fields(
                references,
                table_meta.meta.name.as_str(),
                row,
                fragment.meta.name.as_str(),
                struct_type,
                &record_key,
                source.as_deref(),
                target_tags,
            );
        } else {
            for field in &fragment.fields {
                if !field.meta.is_exported_for(target_tags) {
                    continue;
                }
                let field_path = format!("{}.{}", fragment.meta.name, field.meta.name);
                extract_asset_field_path(
                    references,
                    table_meta.meta.name.as_str(),
                    row,
                    &field_path,
                    field,
                    &record_key,
                    source.as_deref(),
                );
            }
        }
    }
}

fn extract_struct_asset_fields(
    references: &mut Vec<AssetManifestEntry>,
    table_name: &str,
    row: &DataRow,
    parent_path: &str,
    struct_type: &LinkedStructType<'_>,
    record_key: &str,
    source: Option<&str>,
    target_tags: &TargetTags,
) {
    for field in &struct_type.fields {
        if !field.meta.is_exported_for(target_tags) {
            continue;
        }
        let field_path = format!("{parent_path}.{}", field.meta.name);
        match &field.linked_field {
            LinkedField::Struct(child) => extract_struct_asset_fields(
                references,
                table_name,
                row,
                &field_path,
                child,
                record_key,
                source,
                target_tags,
            ),
            _ => extract_asset_field_path(
                references,
                table_name,
                row,
                &field_path,
                field,
                record_key,
                source,
            ),
        }
    }
}

fn extract_asset_field_path(
    references: &mut Vec<AssetManifestEntry>,
    table_name: &str,
    row: &DataRow,
    field_path: &str,
    field: &LinkedFieldMeta<'_>,
    record_key: &str,
    source: Option<&str>,
) {
    if field.primitive_type() != Some(PrimitiveFieldType::Asset) {
        return;
    }
    let Some(asset_kind) = field.meta.asset_kind.as_deref() else {
        return;
    };
    let Some(value) = row.get(field_path).map(String::as_str) else {
        return;
    };
    let path = value.trim();
    if path.is_empty() {
        return;
    }
    references.push(AssetManifestEntry {
        kind: asset_kind.to_owned(),
        path: path.to_owned(),
        referenced_by: vec![AssetReferenceEntry {
            table: table_name.to_owned(),
            row: record_key.to_owned(),
            field: field_path.to_owned(),
            source: source.map(ToOwned::to_owned),
        }],
    });
}

fn build_manifest_entries(
    references: Vec<AssetManifestEntry>,
    options: &AssetManifestOptions,
) -> Vec<AssetManifestEntry> {
    if !options.deduplicate {
        let mut entries = references;
        entries.sort_by(|left, right| {
            (&left.kind, &left.path, &left.referenced_by).cmp(&(
                &right.kind,
                &right.path,
                &right.referenced_by,
            ))
        });
        return entries;
    }

    let mut grouped: BTreeMap<(String, String), BTreeSet<AssetReferenceEntry>> = BTreeMap::new();
    for entry in references {
        grouped
            .entry((entry.kind, entry.path))
            .or_default()
            .extend(entry.referenced_by);
    }
    grouped
        .into_iter()
        .map(|((kind, path), referenced_by)| AssetManifestEntry {
            kind,
            path,
            referenced_by: referenced_by.into_iter().collect(),
        })
        .collect()
}

fn render_json_manifest(
    entries: &[AssetManifestEntry],
    target_tags: &TargetTags,
    options: &AssetManifestOptions,
) -> Result<String> {
    let json_entries = entries
        .iter()
        .map(|entry| JsonAssetEntry {
            kind: entry.kind.clone(),
            path: entry.path.clone(),
            referenced_by: if options.include_referenced_by {
                entry.referenced_by.clone()
            } else {
                Vec::new()
            },
        })
        .collect::<Vec<_>>();
    let manifest = JsonAssetManifest {
        version: 1,
        target_tags: target_tags.iter().map(ToOwned::to_owned).collect(),
        assets: &json_entries,
    };
    let mut output = serde_json::to_string_pretty(&manifest).map_err(|error| {
        ConfigForgeError::Validation(format!("failed to serialize asset manifest JSON: {error}"))
    })?;
    output.push('\n');
    Ok(output)
}

fn render_csv_manifest(
    entries: &[AssetManifestEntry],
    options: &AssetManifestOptions,
) -> Result<String> {
    match options.csv_mode {
        AssetManifestCsvMode::UnrealDatatable => render_unreal_csv_manifest(entries, options),
        AssetManifestCsvMode::Simple => render_simple_csv_manifest(entries),
    }
}

fn render_unreal_csv_manifest(
    entries: &[AssetManifestEntry],
    options: &AssetManifestOptions,
) -> Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer
        .write_record([
            "Name",
            "AssetRef",
            "AssetKind",
            "ReferencedBy",
            "Tables",
            "Fields",
            "RecordKeys",
            "Sources",
        ])
        .map_err(csv_error)?;

    let mut used_names = BTreeMap::<String, usize>::new();
    for entry in entries {
        let row_name = unique_row_name(&mut used_names, &entry.kind, &entry.path);
        let referenced_by = if options.include_referenced_by {
            entry
                .referenced_by
                .iter()
                .map(|reference| {
                    format!("{}[{}].{}", reference.table, reference.row, reference.field)
                })
                .collect::<Vec<_>>()
                .join("|")
        } else {
            String::new()
        };
        let tables = sorted_join(
            entry
                .referenced_by
                .iter()
                .map(|reference| reference.table.as_str()),
        );
        let fields = sorted_join(
            entry
                .referenced_by
                .iter()
                .map(|reference| reference.field.as_str()),
        );
        let record_keys = sorted_join(
            entry
                .referenced_by
                .iter()
                .map(|reference| reference.row.as_str()),
        );
        let sources = sorted_join(
            entry
                .referenced_by
                .iter()
                .filter_map(|reference| reference.source.as_deref()),
        );
        writer
            .write_record([
                row_name.as_str(),
                entry.path.as_str(),
                entry.kind.as_str(),
                referenced_by.as_str(),
                tables.as_str(),
                fields.as_str(),
                record_keys.as_str(),
                sources.as_str(),
            ])
            .map_err(csv_error)?;
    }
    let bytes = writer.into_inner().map_err(|error| {
        ConfigForgeError::Validation(format!("failed to finish asset manifest CSV: {error}"))
    })?;
    String::from_utf8(bytes).map_err(|error| {
        ConfigForgeError::Validation(format!("asset manifest CSV is not valid UTF-8: {error}"))
    })
}

fn render_simple_csv_manifest(entries: &[AssetManifestEntry]) -> Result<String> {
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer.write_record(["AssetRef"]).map_err(csv_error)?;
    for asset_ref in entries
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<BTreeSet<_>>()
    {
        writer.write_record([asset_ref]).map_err(csv_error)?;
    }
    let bytes = writer.into_inner().map_err(|error| {
        ConfigForgeError::Validation(format!("failed to finish asset manifest CSV: {error}"))
    })?;
    String::from_utf8(bytes).map_err(|error| {
        ConfigForgeError::Validation(format!("asset manifest CSV is not valid UTF-8: {error}"))
    })
}

fn csv_error(error: csv::Error) -> ConfigForgeError {
    ConfigForgeError::Validation(format!("failed to write asset manifest CSV: {error}"))
}

fn sorted_join<'a>(values: impl Iterator<Item = &'a str>) -> String {
    values
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join("|")
}

fn unique_row_name(used: &mut BTreeMap<String, usize>, kind: &str, path: &str) -> String {
    let base = unreal_row_name(kind, path);
    let count = used.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}_{}", *count)
    }
}

fn unreal_row_name(kind: &str, path: &str) -> String {
    let mut name = String::new();
    for byte in format!("{kind}_{path}").bytes() {
        if byte.is_ascii_alphanumeric() {
            name.push(byte as char);
        } else if !name.ends_with('_') {
            name.push('_');
        }
    }
    let name = name.trim_matches('_');
    if name.is_empty() || name.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        format!("Asset_{name}")
    } else {
        name.to_owned()
    }
}

fn record_key(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    row_index: usize,
    row: &DataRow,
) -> String {
    match table_meta.meta.kind {
        TableKind::Object => "<object>".to_owned(),
        TableKind::Collection => table_meta
            .primary_index()
            .and_then(|index| {
                index
                    .fields
                    .iter()
                    .map(|field| {
                        row.get(&field.meta.name)
                            .map(|value| format!("{}={value}", field.meta.name))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .map(|parts| parts.join(","))
            .unwrap_or_else(|| {
                table
                    .row_source(row_index)
                    .map(|source| format!("row={}", source.source_row_number))
                    .unwrap_or_else(|| format!("row={}", row_index + 1))
            }),
    }
}

fn source_location(table: &DataTable, row_index: usize) -> Option<String> {
    table.row_source(row_index).map(|source| {
        format!(
            "{}:{}",
            source.source_path.display(),
            source.source_row_number
        )
    })
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use crate::{
        data::{DataSet, DataTable},
        data_artifact::{AggregateDataExporter, TargetDataConfig},
        meta::{MetaModel, TargetTags},
    };

    use super::*;

    fn linked_meta() -> crate::meta::LinkedMetaModel<'static> {
        let meta: &'static MetaModel = Box::leak(Box::new(
            toml::from_str(
                r#"
                [[tables]]
                name = "items"
                kind = "collection"
                primary_index = "by_id"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "asset"
                type = "asset"
                asset_kind = "unreal_skill_class"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "tags"
                type = "string"
                export = false

                [tables.row_export]
                excluded_tags_field = "tags"

                [[tables.indexes]]
                name = "by_id"
                fields = ["id"]
                unique = true
                primary = true

                [[asset_kinds]]
                name = "unreal_skill_class"
                search_paths = ["game"]
                input = "path"
                path_style = "mounted_object"
                extensions = [".uasset"]
                "#,
            )
            .unwrap(),
        ));
        meta.link().unwrap()
    }

    fn data_set() -> DataSet {
        let rows = vec![
            BTreeMap::from([
                ("id".to_owned(), "2".to_owned()),
                (
                    "asset".to_owned(),
                    "/Game/Skills/GA_Fireball.GA_Fireball_C".to_owned(),
                ),
                ("tags".to_owned(), "client".to_owned()),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "1".to_owned()),
                (
                    "asset".to_owned(),
                    "/Game/Skills/GA_Slash.GA_Slash_C".to_owned(),
                ),
            ]),
            BTreeMap::from([
                ("id".to_owned(), "3".to_owned()),
                (
                    "asset".to_owned(),
                    "/Game/Skills/GA_Slash.GA_Slash_C".to_owned(),
                ),
            ]),
        ];
        DataSet::new(vec![DataTable::new("items", rows)])
    }

    fn task_with_options(options: BTreeMap<String, toml::Value>) -> TargetDataConfig {
        TargetDataConfig {
            name: Some("assets".to_owned()),
            output: PathBuf::from("out"),
            format: FORMAT.to_owned(),
            options,
        }
    }

    #[test]
    fn renders_deterministic_json_manifest_for_visible_assets() {
        let linked = linked_meta();
        let task = task_with_options(BTreeMap::new());
        let (_, output) =
            render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set())
                .unwrap();
        assert!(output.contains("\"version\": 1"));
        assert!(output.contains("\"path\": \"/Game/Skills/GA_Slash.GA_Slash_C\""));
        assert!(output.contains("\"row\": \"id=1\""));
        assert!(output.contains("\"row\": \"id=3\""));
        assert!(!output.contains("GA_Fireball"));
        assert_eq!(
            output.matches("/Game/Skills/GA_Slash.GA_Slash_C").count(),
            1
        );
    }

    #[test]
    fn renders_unreal_datatable_csv_manifest() {
        let linked = linked_meta();
        let task = task_with_options(BTreeMap::from([(
            "format".to_owned(),
            toml::Value::String("csv".to_owned()),
        )]));
        let (_, output) =
            render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set())
                .unwrap();
        assert!(output.starts_with(
            "Name,AssetRef,AssetKind,ReferencedBy,Tables,Fields,RecordKeys,Sources\n"
        ));
        assert!(output.contains("unreal_skill_class_Game_Skills_GA_Slash_GA_Slash_C"));
        assert!(output.contains("/Game/Skills/GA_Slash.GA_Slash_C"));
        assert!(output.contains("items[id=1].asset|items[id=3].asset"));
    }

    #[test]
    fn renders_simple_csv_manifest_with_deduplicated_asset_refs_only() {
        let linked = linked_meta();
        let task = task_with_options(BTreeMap::from([
            ("format".to_owned(), toml::Value::String("csv".to_owned())),
            (
                "csv_mode".to_owned(),
                toml::Value::String("simple".to_owned()),
            ),
            ("deduplicate".to_owned(), toml::Value::Boolean(false)),
        ]));
        let (_, output) =
            render_asset_manifest(&task, &linked, &TargetTags::new(["client"]), &data_set())
                .unwrap();
        assert_eq!(output, "AssetRef\n/Game/Skills/GA_Slash.GA_Slash_C\n");
    }

    #[test]
    fn exporter_plans_format_specific_default_file_name() {
        let exporter = AssetManifestExporter;
        let task = task_with_options(BTreeMap::from([(
            "format".to_owned(),
            toml::Value::String("csv".to_owned()),
        )]));
        let paths = AggregateDataExporter::plan_artifacts(
            &exporter,
            Path::new("Generated/Assets"),
            &task,
            &linked_meta(),
            &TargetTags::empty(),
        )
        .unwrap();
        assert_eq!(
            paths,
            [PathBuf::from("Generated/Assets/asset_manifest.csv")]
        );
    }
}
