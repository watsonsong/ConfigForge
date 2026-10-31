// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    Result,
    asset::{AssetResolver, asset_field_paths},
    data::{DataResolver, DataRow, DataSet},
    diagnostic::code::CF_BUILD_EXPORT_TASK_FAILED,
    meta::{AssetsConfig, LinkedMetaModel, TargetTags},
    validation::key_policy::{
        resolved_path_value, scalar_export_value, target_visible_row_indices,
    },
};

pub(super) fn canonicalize_asset_refs(
    meta: &LinkedMetaModel<'_>,
    assets: &AssetsConfig,
    target_tags: &TargetTags,
    data_set: &DataSet,
) -> Result<DataSet> {
    let mut data_set = data_set.clone();
    if meta.model.asset_kinds.is_empty() {
        return Ok(data_set);
    }
    let resolver = AssetResolver::new(meta, assets)?;
    for table in &mut data_set.tables {
        let Some(table_meta) = meta.table(&table.name) else {
            continue;
        };
        if !table_meta.meta.is_exported_for(target_tags) {
            continue;
        }
        let indices = target_visible_row_indices(table_meta, table, target_tags);
        let resolved = DataResolver::resolve_table_rows(
            table_meta,
            table,
            Some(target_tags),
            indices.iter().copied(),
        )?;
        for (row_index, resolved_row) in indices.into_iter().zip(&resolved.rows) {
            let fragments = table.row_sources[row_index].fragments.as_slice();
            let row = &mut table.rows[row_index];
            for (path, field) in asset_field_paths(table_meta, fragments, Some(target_tags)) {
                let Some(kind) = field.meta.asset_kind.as_deref() else {
                    continue;
                };
                let value = scalar_export_value(
                    resolved_path_value(Some(resolved_row), &path),
                    row.get(&path).map(String::as_str),
                );
                if value.trim().is_empty() {
                    continue;
                }
                let canonical = resolver.resolve_for_target(kind, &value, target_tags)?;
                if canonical.canonical_export_ref != value {
                    write_scalar_path(row, &path, canonical.canonical_export_ref)?;
                }
            }
        }
    }
    Ok(data_set)
}

fn write_scalar_path(row: &mut DataRow, path: &str, value: String) -> Result<()> {
    let json_parent = path.rmatch_indices('.').find_map(|(end, _)| {
        row.get(&path[..end])
            .filter(|raw| !raw.trim().is_empty())
            .map(|raw| (end, raw))
    });
    let Some((end, raw)) = json_parent else {
        row.insert(path.to_owned(), value);
        return Ok(());
    };
    // Keep JSON occurrences intact: adding a dotted cell would mix source layouts.
    let invalid_layout = || {
        super::build_diagnostic_error(
            CF_BUILD_EXPORT_TASK_FAILED,
            "build.asset",
            format!("cannot write canonical asset ref at `{path}` into its struct JSON"),
            path,
            "validated struct JSON object",
            "Check struct source layout and asset value preparation.",
        )
    };
    let mut object: serde_json::Value = serde_json::from_str(raw).map_err(|_| invalid_layout())?;
    let mut current = &mut object;
    let mut segments = path[end + 1..].split('.').peekable();
    while let Some(segment) = segments.next() {
        let children = current.as_object_mut().ok_or_else(invalid_layout)?;
        if segments.peek().is_none() {
            children.insert(segment.to_owned(), serde_json::Value::String(value));
            break;
        }
        current = children
            .entry(segment)
            .or_insert_with(|| serde_json::json!({}));
    }
    row.insert(path[..end].to_owned(), object.to_string());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_writeback_preserves_json_siblings_and_expanded_layout() {
        let mut row = DataRow::from([
            ("id".to_owned(), "001".to_owned()),
            (
                "visual".to_owned(),
                r#"{"inner":{},"hidden":"untouched"}"#.to_owned(),
            ),
        ]);
        write_scalar_path(&mut row, "visual.inner.icon", "UI/Icon.png".to_owned()).unwrap();
        assert_eq!(row["id"], "001");
        assert!(!row.contains_key("visual.inner.icon"));
        let value: serde_json::Value = serde_json::from_str(&row["visual"]).unwrap();
        assert_eq!(value["hidden"], "untouched");
        assert_eq!(value["inner"]["icon"], "UI/Icon.png");
        write_scalar_path(&mut row, "other.icon", "UI/Other.png".to_owned()).unwrap();
        assert_eq!(row["other.icon"], "UI/Other.png");
    }

    #[test]
    fn asset_writeback_rejects_invalid_json_or_non_object_ancestors() {
        for raw in ["broken", "[]", r#"{"inner":7}"#] {
            let mut row = DataRow::from([("visual".to_owned(), raw.to_owned())]);
            let error = write_scalar_path(&mut row, "visual.inner.icon", "Icon.png".to_owned())
                .unwrap_err();
            assert_eq!(error.diagnostics()[0].code, CF_BUILD_EXPORT_TASK_FAILED);
            assert_eq!(row["visual"], raw);
        }
    }
}
