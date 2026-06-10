// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::{Result, meta::TargetTags};

use super::{
    AssetManifestEntry, AssetReferenceEntry, asset_manifest_render_error,
    options::{AssetManifestCsvMode, AssetManifestOptions},
};

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

pub(super) fn render_json_manifest(
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
        asset_manifest_render_error(
            format!("failed to serialize asset manifest JSON: {error}"),
            error.to_string(),
            "serializable asset manifest JSON payload",
            "Check asset manifest entries for unsupported JSON serialization data.",
        )
    })?;
    output.push('\n');
    Ok(output)
}

pub(super) fn render_csv_manifest(
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
        asset_manifest_render_error(
            format!("failed to finish asset manifest CSV: {error}"),
            error.to_string(),
            "completed CSV writer buffer",
            "Check asset manifest CSV writer output and underlying buffer state.",
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        asset_manifest_render_error(
            format!("asset manifest CSV is not valid UTF-8: {error}"),
            error.to_string(),
            "UTF-8 asset manifest CSV content",
            "Check asset manifest CSV serialization for non-UTF-8 bytes.",
        )
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
        asset_manifest_render_error(
            format!("failed to finish asset manifest CSV: {error}"),
            error.to_string(),
            "completed CSV writer buffer",
            "Check asset manifest CSV writer output and underlying buffer state.",
        )
    })?;
    String::from_utf8(bytes).map_err(|error| {
        asset_manifest_render_error(
            format!("asset manifest CSV is not valid UTF-8: {error}"),
            error.to_string(),
            "UTF-8 asset manifest CSV content",
            "Check asset manifest CSV serialization for non-UTF-8 bytes.",
        )
    })
}

fn csv_error(error: csv::Error) -> crate::ConfigForgeError {
    asset_manifest_render_error(
        format!("failed to write asset manifest CSV: {error}"),
        error.to_string(),
        "writable asset manifest CSV record",
        "Check asset manifest CSV fields for values accepted by the csv writer.",
    )
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
