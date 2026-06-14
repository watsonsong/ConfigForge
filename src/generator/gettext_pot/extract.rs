// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::{
    Result,
    data::{DataResolver, DataTable, DataValue, ResolvedValue, ScalarValue},
    meta::{LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags},
    validation::key_policy::{raw_record_key, target_visible_row_indices},
};

use super::{
    TextExtractionEntry, gettext_pot_extraction_error,
    options::{GettextPotOptions, PotContextMode, PotSourceReferenceMode},
};

pub(super) fn extract_table_entries(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
    options: &GettextPotOptions,
) -> Result<Vec<TextExtractionEntry>> {
    match table_meta.meta.kind {
        TableKind::Collection => {
            extract_collection_entries(table_meta, table, target_tags, options)
        }
        TableKind::Object => extract_object_entries(table_meta, table, target_tags, options),
    }
}

fn extract_collection_entries(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
    options: &GettextPotOptions,
) -> Result<Vec<TextExtractionEntry>> {
    let visible_row_indices = target_visible_row_indices(table_meta, table, target_tags);
    let resolved_table = DataResolver::resolve_table_rows(
        table_meta,
        table,
        Some(target_tags),
        visible_row_indices.iter().copied(),
    )?;
    let mut entries = Vec::new();
    for (resolved_row_index, row_index) in visible_row_indices.iter().enumerate() {
        let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
            gettext_pot_extraction_error(
                format!(
                    "resolved table `{}` is missing row {}",
                    table_meta.meta.name,
                    resolved_row_index + 1
                ),
                format!("{} row {}", table_meta.meta.name, resolved_row_index + 1),
                "resolved row for every visible source row",
                "Check gettext_pot extraction input after data resolution.",
            )
        })?;
        let record_key = raw_record_key(table_meta, table.rows.get(*row_index));
        for field in table_meta
            .fields
            .iter()
            .filter(|field| field.meta.is_exported_for(target_tags))
        {
            extract_field_entry(
                &mut entries,
                table_meta,
                resolved_row,
                field,
                &field.meta.name,
                record_key.as_deref(),
                options,
            )?;
        }
    }
    Ok(entries)
}

fn extract_object_entries(
    table_meta: &LinkedTableMeta<'_>,
    table: &DataTable,
    target_tags: &TargetTags,
    options: &GettextPotOptions,
) -> Result<Vec<TextExtractionEntry>> {
    if target_visible_row_indices(table_meta, table, target_tags).is_empty() {
        return Ok(Vec::new());
    }
    let resolved_table = DataResolver::resolve_table(table_meta, table, Some(target_tags))?;
    let resolved_row = resolved_table.rows.first().ok_or_else(|| {
        gettext_pot_extraction_error(
            format!(
                "resolved object table `{}` is missing singleton row",
                table_meta.meta.name
            ),
            table_meta.meta.name.clone(),
            "resolved singleton object table row",
            "Check gettext_pot extraction input after object table resolution.",
        )
    })?;
    let mut entries = Vec::new();
    for field in table_meta
        .fields
        .iter()
        .filter(|field| field.meta.is_exported_for(target_tags))
    {
        extract_field_entry(
            &mut entries,
            table_meta,
            resolved_row,
            field,
            &field.meta.name,
            None,
            options,
        )?;
    }
    Ok(entries)
}

fn extract_field_entry(
    entries: &mut Vec<TextExtractionEntry>,
    table_meta: &LinkedTableMeta<'_>,
    resolved_row: &BTreeMap<String, ResolvedValue>,
    field: &LinkedFieldMeta<'_>,
    field_path: &str,
    record_key: Option<&str>,
    options: &GettextPotOptions,
) -> Result<()> {
    if field.primitive_type() != Some(PrimitiveFieldType::Text) {
        return Ok(());
    }
    let Some(value) = resolved_row.get(&field.meta.name) else {
        return Ok(());
    };
    let Some(msgid) = scalar_text(value)? else {
        return Ok(());
    };
    if msgid.is_empty() {
        return Ok(());
    }
    let source_reference = match options.source_reference {
        PotSourceReferenceMode::SourceLocation | PotSourceReferenceMode::Both => {
            value.source.source_path.as_ref().map(|path| {
                if let Some(row) = value.source.source_row_number {
                    format!("{}:{row}", path.display())
                } else {
                    path.display().to_string()
                }
            })
        }
        PotSourceReferenceMode::RecordKey | PotSourceReferenceMode::None => None,
    };
    let mut translator_comments = Vec::new();
    if options.include_translator_comments {
        if let Some(desc) = field.meta.desc.as_deref() {
            translator_comments.push(desc.to_owned());
        }
    }
    translator_comments.push(format!(
        "table={} field={}{}",
        table_meta.meta.name,
        field_path,
        record_key
            .map(|key| format!(" record={key}"))
            .unwrap_or_default()
    ));
    entries.push(TextExtractionEntry {
        msgid,
        msgctxt: msgctxt(&table_meta.meta.name, field_path, options.context),
        table_name: table_meta.meta.name.clone(),
        field_path: field_path.to_owned(),
        record_key: record_key.map(ToOwned::to_owned),
        source_reference,
        translator_comments,
    });
    Ok(())
}

fn scalar_text(value: &ResolvedValue) -> Result<Option<String>> {
    match &value.value {
        DataValue::Scalar(ScalarValue::String(_)) => Ok(value.scalar_export_string()),
        DataValue::Scalar(_) => Err(gettext_pot_extraction_error(
            "text field resolved to non-string scalar",
            format!("{:?}", value.value),
            "string scalar for text field",
            "Check text field type resolution before gettext_pot extraction.",
        )),
        _ => Ok(None),
    }
}

fn msgctxt(table: &str, field_path: &str, mode: PotContextMode) -> Option<String> {
    match mode {
        PotContextMode::None => None,
        PotContextMode::TableField => Some(format!("{table}.{field_path}")),
        PotContextMode::Field => Some(field_path.to_owned()),
    }
}
