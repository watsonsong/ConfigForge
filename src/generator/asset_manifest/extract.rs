// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    asset::asset_field_paths,
    data::{DataResolver, DataRow, DataTable, ResolvedRow},
    meta::{LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags},
    validation::key_policy::{
        raw_record_key_or_row_fallback, resolved_path_value, scalar_export_value,
        target_visible_row_indices,
    },
};

use super::{AssetManifestEntry, AssetReferenceEntry, options::AssetManifestOptions};

pub(super) fn extract_table_asset_references(
    references: &mut Vec<AssetManifestEntry>,
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
    options: &AssetManifestOptions,
) {
    let visible_row_indices = target_visible_row_indices(table_meta, table, target_tags);
    let resolved_table = DataResolver::resolve_table_rows(
        table_meta,
        table,
        Some(target_tags),
        visible_row_indices.iter().copied(),
    )
    .ok();

    match table_meta.meta.kind {
        TableKind::Collection => {
            for (visible_row_index, row_index) in visible_row_indices.iter().enumerate() {
                let Some(row) = table.rows.get(*row_index) else {
                    continue;
                };
                extract_row_asset_references(
                    references,
                    table_meta,
                    table,
                    *row_index,
                    row,
                    resolved_table
                        .as_ref()
                        .and_then(|resolved_table| resolved_table.rows.get(visible_row_index)),
                    target_tags,
                    options,
                );
            }
        }
        TableKind::Object => {
            if !visible_row_indices.is_empty()
                && let Some(row) = table.rows.first()
            {
                extract_row_asset_references(
                    references,
                    table_meta,
                    table,
                    0,
                    row,
                    resolved_table
                        .as_ref()
                        .and_then(|resolved_table| resolved_table.rows.first()),
                    target_tags,
                    options,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_row_asset_references(
    references: &mut Vec<AssetManifestEntry>,
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    row_index: usize,
    row: &DataRow,
    resolved_row: Option<&ResolvedRow>,
    target_tags: &TargetTags,
    options: &AssetManifestOptions,
) {
    let record_key = raw_record_key_or_row_fallback(table_meta, table, row_index, row);
    let active_fragments = table
        .row_source(row_index)
        .map(|source| source.fragments.as_slice())
        .unwrap_or(&[]);
    for (field_path, field) in asset_field_paths(table_meta, active_fragments, Some(target_tags)) {
        let source = options
            .include_source_location
            .then(|| source_location(table, row_index, &field_path))
            .flatten();
        extract_asset_field_path(
            references,
            table_meta.meta.name.as_str(),
            row,
            resolved_row,
            &field_path,
            field,
            &record_key,
            source.as_deref(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn extract_asset_field_path(
    references: &mut Vec<AssetManifestEntry>,
    table_name: &str,
    row: &DataRow,
    resolved_row: Option<&ResolvedRow>,
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
    let value = scalar_export_value(
        resolved_path_value(resolved_row, field_path),
        raw_cell(row, field_path),
    );
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

fn raw_cell<'a>(row: &'a DataRow, field_path: &str) -> Option<&'a str> {
    row.get(field_path).map(String::as_str)
}

pub(super) fn build_manifest_entries(
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

fn source_location(table: &DataTable, row_index: usize, field_path: &str) -> Option<String> {
    let mut location = crate::DiagnosticLocation {
        field: Some(field_path.to_owned()),
        ..Default::default()
    };
    table.enrich_location(row_index, &mut location);
    Some(format!(
        "{}:{}",
        location.source_path?.display(),
        location.source_row_number?
    ))
}
