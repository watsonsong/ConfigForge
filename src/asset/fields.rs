// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use crate::meta::{LinkedField, LinkedFieldMeta, LinkedTableMeta, PrimitiveFieldType, TargetTags};

pub(crate) fn asset_field_paths<'a, 'meta>(
    table: &'a LinkedTableMeta<'meta>,
    active_fragments: &[String],
    target_tags: Option<&TargetTags>,
) -> Vec<(String, &'a LinkedFieldMeta<'meta>)> {
    let mut paths = Vec::new();
    collect_fields(&table.fields, "", target_tags, &mut paths);
    for name in active_fragments {
        let Some(fragment) = table.fragment(name) else {
            continue;
        };
        if target_tags.is_some_and(|tags| !fragment.meta.is_exported_for(tags)) {
            continue;
        }
        let fields = fragment
            .struct_type()
            .map(|value| value.fields.as_slice())
            .unwrap_or(&fragment.fields);
        collect_fields(fields, name, target_tags, &mut paths);
    }
    paths
}

fn collect_fields<'a, 'meta>(
    fields: &'a [LinkedFieldMeta<'meta>],
    parent: &str,
    target_tags: Option<&TargetTags>,
    paths: &mut Vec<(String, &'a LinkedFieldMeta<'meta>)>,
) {
    for field in fields {
        if target_tags.is_some_and(|tags| !field.meta.is_exported_for(tags)) {
            continue;
        }
        let path = if parent.is_empty() {
            field.meta.name.clone()
        } else {
            format!("{parent}.{}", field.meta.name)
        };
        match &field.linked_field {
            LinkedField::Struct(value) => collect_fields(&value.fields, &path, target_tags, paths),
            LinkedField::Primitive(PrimitiveFieldType::Asset) => paths.push((path, field)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::MetaModel;

    #[test]
    fn full_asset_paths_include_hidden_children_and_target_paths_filter_ancestors() {
        let meta = MetaModel::from_toml_str(
            r#"
            [[asset_kinds]]
            name = "icon"
            [[structs]]
            name = "Visual"
            [[structs.fields]]
            name = "icon"
            type = "asset"
            asset_kind = "icon"
            [[structs.fields]]
            name = "hidden"
            type = "asset"
            asset_kind = "icon"
            export = false
            [[tables]]
            name = "items"
            source = "items.csv"
            [[tables.fields]]
            name = "visual"
            type = "Visual"
            [[tables.fields]]
            name = "server"
            type = "Visual"
            export_when = { any = ["server"] }
            "#,
        )
        .unwrap();
        let linked = meta.link().unwrap();
        let paths = |tags| {
            asset_field_paths(&linked.tables[0], &[], tags)
                .into_iter()
                .map(|(path, _)| path)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(None),
            [
                "visual.icon",
                "visual.hidden",
                "server.icon",
                "server.hidden"
            ]
        );
        assert_eq!(paths(Some(&TargetTags::new(["client"]))), ["visual.icon"]);
    }
}
