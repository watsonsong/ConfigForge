// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::PathBuf};

use serde::Deserialize;

use crate::{
    ConfigForgeError, Result,
    artifact::Artifact,
    data::{DataResolver, DataSet, DataTable, DataValue, ResolvedValue, ScalarValue},
    meta::{LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType, TableKind, TargetTags},
};

use super::{AggregateDataExporter, TargetDataConfig};

pub const FORMAT: &str = "gettext_pot";

pub struct GettextPotExporter;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct GettextPotOptions {
    domain: Option<String>,
    file_name: Option<String>,
    context: PotContextMode,
    source_reference: PotSourceReferenceMode,
    include_translator_comments: bool,
    deduplicate: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PotContextMode {
    None,
    TableField,
    Field,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PotSourceReferenceMode {
    SourceLocation,
    RecordKey,
    Both,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextExtractionEntry {
    msgid: String,
    msgctxt: Option<String>,
    table_name: String,
    field_path: String,
    record_key: Option<String>,
    source_reference: Option<String>,
    translator_comments: Vec<String>,
}

impl Default for GettextPotOptions {
    fn default() -> Self {
        Self {
            domain: None,
            file_name: None,
            context: PotContextMode::TableField,
            source_reference: PotSourceReferenceMode::SourceLocation,
            include_translator_comments: true,
            deduplicate: true,
        }
    }
}

impl AggregateDataExporter for GettextPotExporter {
    fn name(&self) -> &'static str {
        FORMAT
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["pot"]
    }

    fn plan_artifacts(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        _linked_meta: &crate::meta::LinkedMetaModel<'_>,
        _target_tags: &TargetTags,
    ) -> Result<Vec<PathBuf>> {
        let options = options_from_task(task)?;
        Ok(vec![out_dir.join(options.file_name())])
    }

    fn export(
        &self,
        out_dir: &std::path::Path,
        task: &TargetDataConfig,
        linked_meta: &crate::meta::LinkedMetaModel<'_>,
        target_tags: &TargetTags,
        data_set: &DataSet,
    ) -> Result<Vec<Artifact>> {
        let options = options_from_task(task)?;
        let mut entries = Vec::new();
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
            entries.extend(extract_table_entries(
                table_meta,
                table,
                target_tags,
                &options,
            )?);
        }
        let content = render_pot(&entries, &options);
        Ok(vec![Artifact::text(
            out_dir.join(options.file_name()),
            content,
        )])
    }
}

impl GettextPotOptions {
    fn file_name(&self) -> String {
        self.file_name.clone().unwrap_or_else(|| {
            format!(
                "{}.pot",
                self.domain
                    .as_deref()
                    .unwrap_or("messages")
                    .replace('/', "_")
            )
        })
    }
}

fn options_from_task(task: &TargetDataConfig) -> Result<GettextPotOptions> {
    if let Some(value) = task.options.get("gettext") {
        return value.clone().try_into().map_err(|error| {
            ConfigForgeError::Validation(format!("invalid gettext_pot options: {error}"))
        });
    }
    if task.options.is_empty() {
        return Ok(GettextPotOptions::default());
    }
    toml::Value::Table(task.options.clone().into_iter().collect())
        .try_into()
        .map_err(|error| {
            ConfigForgeError::Validation(format!("invalid gettext_pot options: {error}"))
        })
}

fn extract_table_entries(
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
    let mut entries = Vec::new();
    for (resolved_row_index, row_index) in visible_row_indices.iter().enumerate() {
        let resolved_row = resolved_table.rows.get(resolved_row_index).ok_or_else(|| {
            ConfigForgeError::Validation(format!(
                "resolved table `{}` is missing row {}",
                table_meta.meta.name,
                resolved_row_index + 1
            ))
        })?;
        let record_key = record_key(table_meta, table.rows.get(*row_index));
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
    let Some(row) = table.rows.first() else {
        return Ok(Vec::new());
    };
    if !table_meta
        .meta
        .is_row_exported_for(target_tags, |field| row.get(field).map(String::as_str))
    {
        return Ok(Vec::new());
    }
    let resolved_table = DataResolver::resolve_table(table_meta, table, Some(target_tags))?;
    let resolved_row = resolved_table.rows.first().ok_or_else(|| {
        ConfigForgeError::Validation(format!(
            "resolved object table `{}` is missing singleton row",
            table_meta.meta.name
        ))
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
        DataValue::Scalar(_) => Err(ConfigForgeError::Validation(
            "text field resolved to non-string scalar".to_owned(),
        )),
        _ => Ok(None),
    }
}

fn record_key(
    table_meta: &LinkedTableMeta<'_>,
    row: Option<&BTreeMap<String, String>>,
) -> Option<String> {
    let row = row?;
    let index = table_meta.primary_index()?;
    let parts = index
        .fields
        .iter()
        .map(|field| {
            row.get(&field.meta.name)
                .map(|value| format!("{}={}", field.meta.name, value))
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join(","))
}

fn msgctxt(table: &str, field_path: &str, mode: PotContextMode) -> Option<String> {
    match mode {
        PotContextMode::None => None,
        PotContextMode::TableField => Some(format!("{table}.{field_path}")),
        PotContextMode::Field => Some(field_path.to_owned()),
    }
}

fn render_pot(entries: &[TextExtractionEntry], options: &GettextPotOptions) -> String {
    let entries = if options.deduplicate {
        deduplicate_entries(entries)
    } else {
        entries.to_vec()
    };
    let mut output = String::new();
    output.push_str("msgid \"\"\n");
    output.push_str("msgstr \"\"\n");
    let project_id = options.domain.as_deref().unwrap_or("messages");
    output.push_str(&format!(
        "\"Project-Id-Version: {}\\n\"\n",
        escape_po(project_id)
    ));
    output.push_str("\"Content-Type: text/plain; charset=UTF-8\\n\"\n");
    output.push_str("\"Content-Transfer-Encoding: 8bit\\n\"\n\n");

    for entry in entries {
        for comment in &entry.translator_comments {
            output.push_str(&format!("#. {}\n", comment));
        }
        if let Some(source_reference) = &entry.source_reference {
            output.push_str(&format!("#: {}\n", source_reference));
        }
        if let Some(context) = &entry.msgctxt {
            output.push_str(&format!("msgctxt \"{}\"\n", escape_po(context)));
        }
        output.push_str(&format!("msgid \"{}\"\n", escape_po(&entry.msgid)));
        output.push_str("msgstr \"\"\n\n");
    }
    output
}

fn deduplicate_entries(entries: &[TextExtractionEntry]) -> Vec<TextExtractionEntry> {
    let mut map: BTreeMap<(Option<String>, String), TextExtractionEntry> = BTreeMap::new();
    for entry in entries {
        let key = (entry.msgctxt.clone(), entry.msgid.clone());
        if let Some(existing) = map.get_mut(&key) {
            merge_unique(
                &mut existing.translator_comments,
                &entry.translator_comments,
            );
            if existing.source_reference != entry.source_reference {
                match (&existing.source_reference, &entry.source_reference) {
                    (Some(left), Some(right)) => {
                        existing.source_reference = Some(format!("{left} {right}"));
                    }
                    (None, Some(right)) => existing.source_reference = Some(right.clone()),
                    _ => {}
                }
            }
        } else {
            map.insert(key, entry.clone());
        }
    }
    map.into_values().collect()
}

fn merge_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

fn escape_po(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_po_strings() {
        assert_eq!(escape_po("a\\b\"c\n"), "a\\\\b\\\"c\\n");
    }

    #[test]
    fn deduplicates_by_context_and_msgid() {
        let entries = vec![
            TextExtractionEntry {
                msgid: "Iron Ore".to_owned(),
                msgctxt: Some("items.name".to_owned()),
                table_name: "items".to_owned(),
                field_path: "name".to_owned(),
                record_key: Some("id=1".to_owned()),
                source_reference: Some("items.csv:2".to_owned()),
                translator_comments: vec!["Name".to_owned()],
            },
            TextExtractionEntry {
                msgid: "Iron Ore".to_owned(),
                msgctxt: Some("items.name".to_owned()),
                table_name: "items".to_owned(),
                field_path: "name".to_owned(),
                record_key: Some("id=2".to_owned()),
                source_reference: Some("items.csv:3".to_owned()),
                translator_comments: vec!["Name".to_owned(), "Second".to_owned()],
            },
        ];

        let deduped = deduplicate_entries(&entries);

        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].translator_comments, vec!["Name", "Second"]);
        assert_eq!(
            deduped[0].source_reference.as_deref(),
            Some("items.csv:2 items.csv:3")
        );
    }
}
