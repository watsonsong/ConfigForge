// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    data::{DataRow, DataTable},
    meta::{
        LinkedField, LinkedFieldMeta, LinkedStructType, LinkedTableMeta, PrimitiveFieldType,
        TableKind, TargetTags,
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
            let context = AssetExtractionContext {
                table_name: table_meta.meta.name.as_str(),
                row,
                record_key: &record_key,
                source: source.as_deref(),
                target_tags,
            };
            extract_struct_asset_fields(
                references,
                fragment.meta.name.as_str(),
                struct_type,
                context,
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

#[derive(Clone, Copy)]
struct AssetExtractionContext<'a> {
    table_name: &'a str,
    row: &'a DataRow,
    record_key: &'a str,
    source: Option<&'a str>,
    target_tags: &'a TargetTags,
}

fn extract_struct_asset_fields(
    references: &mut Vec<AssetManifestEntry>,
    parent_path: &str,
    struct_type: &LinkedStructType<'_>,
    context: AssetExtractionContext<'_>,
) {
    for field in &struct_type.fields {
        if !field.meta.is_exported_for(context.target_tags) {
            continue;
        }
        let field_path = format!("{parent_path}.{}", field.meta.name);
        match &field.linked_field {
            LinkedField::Struct(child) => {
                extract_struct_asset_fields(references, &field_path, child, context)
            }
            _ => extract_asset_field_path(
                references,
                context.table_name,
                context.row,
                &field_path,
                field,
                context.record_key,
                context.source,
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
