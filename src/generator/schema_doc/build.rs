// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use crate::meta::{
    AssetKindMeta, ExportCondition, LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta,
    LinkedFragmentMeta, LinkedFragmentPayload, LinkedMetaModel, LinkedStructType, LinkedTableMeta,
    PrimitiveFieldType, SourceRef, TableKind, ValidatorMeta,
};

use super::model::{
    SchemaAssetFieldDoc, SchemaDocOptions, SchemaFieldDoc, SchemaFragmentDoc,
    SchemaFragmentFieldDoc, SchemaFragmentGraphDoc, SchemaIndexDoc, SchemaRelationDoc,
    SchemaRelationEdgeDoc, SchemaRelationshipGraph, SchemaSourceGraph, SchemaSourceNodeDoc,
    SchemaSourceTableEdgeDoc, SchemaTableDoc, SchemaTableNodeDoc, SchemaTextFieldDoc,
    SchemaValidatorDoc,
};

pub(super) fn table_doc(
    linked_meta: &LinkedMetaModel<'_>,
    table: &LinkedTableMeta<'_>,
    options: &SchemaDocOptions,
) -> Option<SchemaTableDoc> {
    let table_visible = table.meta.is_exported_for(&options.target_tags);
    if !is_documented_table(
        table,
        &options.target_tags,
        options.include_internal,
        options.include_hidden,
    ) {
        return None;
    }
    let fields = field_docs(table, options);
    let indexes = index_docs(table, &fields, options);
    let relations = relation_docs(linked_meta, table, &fields, options);
    let asset_fields = fields
        .iter()
        .filter_map(|field| {
            field
                .asset_kind
                .as_ref()
                .map(|asset_kind| SchemaAssetFieldDoc {
                    field: field.path.clone(),
                    asset_kind: Some(asset_kind.clone()),
                    required: field.required,
                    visibility: field.visibility.clone(),
                    rules: options
                        .include_asset_rules
                        .then(|| linked_meta.asset_kind(asset_kind))
                        .flatten()
                        .map(asset_kind_rules),
                })
        })
        .collect::<Vec<_>>();
    let text_fields = fields
        .iter()
        .filter(|field| field.enters_pot)
        .map(|field| SchemaTextFieldDoc {
            field: field.path.clone(),
            visibility: field.visibility.clone(),
            enters_pot: true,
        })
        .collect::<Vec<_>>();
    let validators = table
        .meta
        .validators
        .iter()
        .filter(|validator| validator.export_when.matches(&options.target_tags))
        .map(|validator| validator_doc("table", validator))
        .collect();
    Some(SchemaTableDoc {
        name: table.meta.name.clone(),
        kind: table_kind_display(table.meta.kind).to_owned(),
        description: table.meta.desc.clone(),
        source_summary: source_summary(table),
        visibility: export_visibility(table.meta.export, &table.meta.export_when, table_visible),
        row_export_summary: if table.meta.row_export.is_some() {
            "yes".to_owned()
        } else {
            "no".to_owned()
        },
        primary_index: table.primary_index().map(index_signature),
        fields,
        indexes,
        relations,
        incoming_relations: Vec::new(),
        fragment_graph: fragment_graph_doc(table, options),
        asset_fields,
        text_fields,
        validators,
    })
}

pub(super) fn is_documented_table(
    table: &LinkedTableMeta<'_>,
    tags: &crate::meta::TargetTags,
    include_internal: bool,
    include_hidden: bool,
) -> bool {
    if !table.meta.export {
        include_internal
    } else {
        table.meta.export_when.matches(tags) || include_hidden
    }
}

fn fragment_graph_doc(
    table: &LinkedTableMeta<'_>,
    options: &SchemaDocOptions,
) -> Option<SchemaFragmentGraphDoc> {
    if !options.include_struct_details {
        return None;
    }
    let fragments = table
        .fragments
        .iter()
        .filter_map(|fragment| fragment_doc(table, fragment, options))
        .collect::<Vec<_>>();
    if fragments.is_empty() {
        return None;
    }

    Some(SchemaFragmentGraphDoc {
        table: table.meta.name.clone(),
        base_fields: table
            .fields
            .iter()
            .filter(|field| is_documented_field(field, options))
            .map(|field| field.meta.name.clone())
            .collect(),
        fragments,
    })
}

fn fragment_doc(
    table: &LinkedTableMeta<'_>,
    fragment: &LinkedFragmentMeta<'_>,
    options: &SchemaDocOptions,
) -> Option<SchemaFragmentDoc> {
    let fragment_visible = fragment.meta.is_exported_for(&options.target_tags);
    if !is_documented_exportable(
        fragment.meta.export,
        fragment.meta.export_when.matches(&options.target_tags),
        options,
    ) {
        return None;
    }
    let mut fields = Vec::new();
    for field in &fragment.fields {
        collect_fragment_field_doc(&mut fields, field, None, options);
    }
    Some(SchemaFragmentDoc {
        name: fragment.meta.name.clone(),
        description: fragment.meta.desc.clone(),
        visibility: export_visibility(
            fragment.meta.export,
            &fragment.meta.export_when,
            fragment_visible,
        ),
        payload: fragment_payload_display(fragment),
        source_hints: fragment_source_hints(table, &fragment.meta.name, options),
        fields,
    })
}

fn collect_fragment_field_doc(
    fields: &mut Vec<SchemaFragmentFieldDoc>,
    field: &LinkedFieldMeta<'_>,
    prefix: Option<&str>,
    options: &SchemaDocOptions,
) {
    if !is_documented_field(field, options) {
        return;
    }
    let path = prefix
        .map(|prefix| format!("{prefix}.{}", field.meta.name))
        .unwrap_or_else(|| field.meta.name.clone());
    let field_visible = field.meta.is_exported_for(&options.target_tags);
    fields.push(SchemaFragmentFieldDoc {
        path: path.clone(),
        type_display: field_type_display(field),
        visibility: export_visibility(field.meta.export, &field.meta.export_when, field_visible),
        description: field.meta.desc.clone(),
    });
    if options.include_struct_details {
        if let LinkedField::Struct(struct_type) = &field.linked_field {
            for child in &struct_type.fields {
                collect_fragment_field_doc(fields, child, Some(&path), options);
            }
        }
    }
}

fn is_documented_field(field: &LinkedFieldMeta<'_>, options: &SchemaDocOptions) -> bool {
    is_documented_exportable(
        field.meta.export,
        field.meta.export_when.matches(&options.target_tags),
        options,
    )
}

fn is_documented_exportable(
    export: bool,
    export_when_matches: bool,
    options: &SchemaDocOptions,
) -> bool {
    if !export {
        options.include_internal
    } else {
        export_when_matches || options.include_hidden
    }
}

fn fragment_payload_display(fragment: &LinkedFragmentMeta<'_>) -> String {
    match &fragment.payload {
        LinkedFragmentPayload::InlineFields => "inline fields".to_owned(),
        LinkedFragmentPayload::Struct(struct_type) => format!("struct({})", struct_type.meta.name),
    }
}

fn fragment_source_hints(
    table: &LinkedTableMeta<'_>,
    fragment_name: &str,
    options: &SchemaDocOptions,
) -> Vec<String> {
    let mut hints = Vec::new();
    for (role, source) in source_refs_for_table(table) {
        if !source
            .fragments
            .iter()
            .any(|fragment| fragment == fragment_name)
        {
            continue;
        }
        if role == "override"
            && !source.export_when.matches(&options.target_tags)
            && !options.include_hidden
        {
            continue;
        }
        hints.push(format!("{} ({role})", source.path));
    }
    hints
}

fn source_refs_for_table<'meta>(
    table: &LinkedTableMeta<'meta>,
) -> Vec<(&'static str, &'meta SourceRef)> {
    let mut sources = Vec::new();
    if table.meta.sources.is_empty() {
        if let Some(source) = &table.meta.source {
            sources.push(("base", source));
        }
    } else {
        sources.extend(table.meta.sources.iter().map(|source| ("base", source)));
    }
    sources.extend(
        table
            .meta
            .override_sources
            .iter()
            .map(|source| ("override", source)),
    );
    sources
}

fn field_docs(table: &LinkedTableMeta<'_>, options: &SchemaDocOptions) -> Vec<SchemaFieldDoc> {
    let mut fields = Vec::new();
    for field in &table.fields {
        collect_field_doc(&mut fields, field, None, options);
    }
    if options.include_struct_details {
        for fragment in &table.fragments {
            if !is_documented_exportable(
                fragment.meta.export,
                fragment.meta.export_when.matches(&options.target_tags),
                options,
            ) {
                continue;
            }
            for field in &fragment.fields {
                collect_field_doc(&mut fields, field, Some(&fragment.meta.name), options);
            }
        }
    }
    fields
}

fn collect_field_doc(
    fields: &mut Vec<SchemaFieldDoc>,
    field: &LinkedFieldMeta<'_>,
    prefix: Option<&str>,
    options: &SchemaDocOptions,
) {
    let field_visible = field.meta.is_exported_for(&options.target_tags);
    if !is_documented_field(field, options) {
        return;
    }
    let path = prefix
        .map(|prefix| format!("{prefix}.{}", field.meta.name))
        .unwrap_or_else(|| field.meta.name.clone());
    let type_display = field_type_display(field);
    let visibility = export_visibility(field.meta.export, &field.meta.export_when, field_visible);
    let is_text = matches!(
        field.linked_field.primitive_type(),
        Some(PrimitiveFieldType::Text)
    );
    fields.push(SchemaFieldDoc {
        path: path.clone(),
        type_display,
        required: field.meta.required,
        default_display: field.meta.default.as_ref().map(toml_value_display),
        visibility: visibility.clone(),
        description: field.meta.desc.clone(),
        asset_kind: field.meta.asset_kind.clone(),
        enters_pot: options.include_i18n && is_text && field_visible && field.meta.export,
        validators: field
            .meta
            .validators
            .iter()
            .filter(|validator| validator.export_when.matches(&options.target_tags))
            .map(|validator| validator_doc(&path, validator))
            .collect(),
    });
    if options.include_struct_details {
        if let LinkedField::Struct(struct_type) = &field.linked_field {
            collect_struct_children(fields, struct_type, &path, options);
        }
    }
}

fn collect_struct_children(
    fields: &mut Vec<SchemaFieldDoc>,
    struct_type: &LinkedStructType<'_>,
    parent: &str,
    options: &SchemaDocOptions,
) {
    for child in &struct_type.fields {
        collect_field_doc(fields, child, Some(parent), options);
    }
}

fn index_docs(
    table: &LinkedTableMeta<'_>,
    fields: &[SchemaFieldDoc],
    options: &SchemaDocOptions,
) -> Vec<SchemaIndexDoc> {
    let visible_fields = fields
        .iter()
        .map(|field| field.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    table
        .indexes
        .iter()
        .filter_map(|index| {
            let fields_documented = index
                .fields
                .iter()
                .all(|field| visible_fields.contains(field.meta.name.as_str()));
            let index_export_matches = index.meta.export_when.matches(&options.target_tags);
            let index_visible =
                index.meta.is_exported_for(&options.target_tags) && fields_documented;
            if !fields_documented
                || !is_documented_exportable(index.meta.export, index_export_matches, options)
            {
                return None;
            }
            Some(SchemaIndexDoc {
                name: index.meta.name.clone(),
                fields: index
                    .fields
                    .iter()
                    .map(|field| field.meta.name.clone())
                    .collect(),
                unique: index.meta.unique,
                primary: table
                    .primary_index()
                    .is_some_and(|primary| primary.meta.name == index.meta.name),
                visibility: export_visibility(
                    index.meta.export,
                    &index.meta.export_when,
                    index_visible,
                ),
                description: index.meta.desc.clone(),
            })
        })
        .collect()
}

fn relation_docs(
    linked_meta: &LinkedMetaModel<'_>,
    table: &LinkedTableMeta<'_>,
    fields: &[SchemaFieldDoc],
    options: &SchemaDocOptions,
) -> Vec<SchemaRelationDoc> {
    let visible_fields = fields
        .iter()
        .map(|field| field.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    table
        .foreign_keys
        .iter()
        .filter_map(|foreign_key| {
            let target_table_documented = linked_meta
                .table(foreign_key.target_table_name)
                .is_some_and(|target_table| {
                    is_documented_exportable(
                        target_table.meta.export,
                        target_table.meta.export_when.matches(&options.target_tags),
                        options,
                    )
                });
            let target_table_visible = linked_meta
                .table(foreign_key.target_table_name)
                .is_some_and(|target_table| {
                    target_table.meta.is_exported_for(&options.target_tags)
                });
            let source_fields_documented = foreign_key
                .fields
                .iter()
                .all(|field| visible_fields.contains(field.meta.name.as_str()));
            let relation_visible = foreign_key.is_exported_for(&options.target_tags)
                && target_table_visible
                && source_fields_documented;
            if !target_table_documented
                || !source_fields_documented
                || !is_documented_exportable(
                    foreign_key.export,
                    foreign_key.export_when.matches(&options.target_tags),
                    options,
                )
            {
                return None;
            }
            Some(SchemaRelationDoc {
                field: foreign_key
                    .fields
                    .iter()
                    .map(|field| field.meta.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                target_table: foreign_key.target_table_name.to_owned(),
                target_index: foreign_key.target_index_name.to_owned(),
                target: relation_target_display(linked_meta, foreign_key),
                cardinality: relation_cardinality(foreign_key).to_owned(),
                allow_empty: foreign_key.allow_empty,
                linked_helper: foreign_key.linked_field.map(str::to_owned),
                description: foreign_key.desc.map(str::to_owned),
                visibility: export_visibility(
                    foreign_key.export,
                    foreign_key.export_when,
                    relation_visible,
                ),
            })
        })
        .collect()
}

pub(super) fn relationship_graph(tables: &[SchemaTableDoc]) -> SchemaRelationshipGraph {
    let nodes = tables
        .iter()
        .map(|table| SchemaTableNodeDoc {
            table: table.name.clone(),
            kind: table.kind.clone(),
            description: table.description.clone(),
            source_summary: table.source_summary.clone(),
            primary_index: table.primary_index.clone(),
            field_count: table.fields.len(),
            index_count: table.indexes.len(),
            relation_count: table.relations.len(),
            visibility: table.visibility.clone(),
        })
        .collect();
    let edges = tables
        .iter()
        .flat_map(|table| {
            table
                .relations
                .iter()
                .map(|relation| SchemaRelationEdgeDoc {
                    source_table: table.name.clone(),
                    source_field: relation.field.clone(),
                    target_table: relation.target_table.clone(),
                    target_index: relation.target_index.clone(),
                    target_display: relation.target.clone(),
                    cardinality: relation.cardinality.clone(),
                    allow_empty: relation.allow_empty,
                    linked_helper: relation.linked_helper.clone(),
                    visibility: relation.visibility.clone(),
                    description: relation.description.clone(),
                })
        })
        .collect();
    SchemaRelationshipGraph { nodes, edges }
}

pub(super) fn apply_incoming_relations(
    tables: &mut [SchemaTableDoc],
    edges: &[SchemaRelationEdgeDoc],
) {
    for table in tables {
        table.incoming_relations = edges
            .iter()
            .filter(|edge| edge.target_table == table.name)
            .cloned()
            .collect();
    }
}

pub(super) fn source_graph(
    linked_meta: &LinkedMetaModel<'_>,
    tables: &[SchemaTableDoc],
    options: &SchemaDocOptions,
) -> SchemaSourceGraph {
    let visible_tables = tables
        .iter()
        .map(|table| table.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut edges = Vec::new();
    let mut seen_sources = BTreeSet::new();
    let mut nodes = Vec::new();

    for table in &linked_meta.tables {
        if !visible_tables.contains(table.meta.name.as_str()) {
            continue;
        }
        for edge in source_edges_for_table(table, options) {
            if seen_sources.insert((edge.provider.clone(), edge.source_path.clone())) {
                nodes.push(SchemaSourceNodeDoc {
                    path: edge.source_path.clone(),
                    provider: edge.provider.clone(),
                });
            }
            edges.push(edge);
        }
    }

    SchemaSourceGraph { nodes, edges }
}

fn source_edges_for_table(
    table: &LinkedTableMeta<'_>,
    options: &SchemaDocOptions,
) -> Vec<SchemaSourceTableEdgeDoc> {
    let mut edges = Vec::new();
    if table.meta.sources.is_empty() {
        if let Some(source) = &table.meta.source {
            edges.push(source_edge_doc(table, source, "base", options));
        }
    } else {
        edges.extend(
            table
                .meta
                .sources
                .iter()
                .map(|source| source_edge_doc(table, source, "base", options)),
        );
    }
    edges.extend(table.meta.override_sources.iter().filter_map(|source| {
        let visible = source.export_when.matches(&options.target_tags);
        (visible || options.include_hidden)
            .then(|| source_edge_doc(table, source, "override", options))
    }));
    edges
}

fn source_edge_doc(
    table: &LinkedTableMeta<'_>,
    source: &SourceRef,
    role: &str,
    options: &SchemaDocOptions,
) -> SchemaSourceTableEdgeDoc {
    let source_visible = source.export_when.matches(&options.target_tags);
    let table_visible = table.meta.is_exported_for(&options.target_tags);
    let visibility = if !table.meta.export {
        "internal".to_owned()
    } else if !table_visible {
        "hidden".to_owned()
    } else {
        export_visibility(true, &source.export_when, source_visible)
    };
    SchemaSourceTableEdgeDoc {
        source_path: source.path.clone(),
        provider: source.provider.clone(),
        table: table.meta.name.clone(),
        role: role.to_owned(),
        fragments: source.fragments.clone(),
        layout: source_layout_display(table),
        visibility,
        header_row: source.header_row,
        data_start_row: object_table_data_start_row(table, source),
    }
}

fn source_layout_display(table: &LinkedTableMeta<'_>) -> String {
    match table.meta.kind {
        TableKind::Object => "key/value rows".to_owned(),
        TableKind::Collection => "collection records".to_owned(),
    }
}

fn object_table_data_start_row(table: &LinkedTableMeta<'_>, source: &SourceRef) -> usize {
    if matches!(table.meta.kind, TableKind::Object) && source.data_start_row == 3 {
        2
    } else {
        source.data_start_row
    }
}

fn relation_target_display(
    linked_meta: &LinkedMetaModel<'_>,
    foreign_key: &LinkedForeignKeyMeta<'_>,
) -> String {
    let Some(target_table) = linked_meta.table(foreign_key.target_table_name) else {
        return format!(
            "{}({})",
            foreign_key.target_table_name, foreign_key.target_index_name
        );
    };
    let Some(target_index) = target_table.index(foreign_key.target_index_name) else {
        return format!(
            "{}({})",
            foreign_key.target_table_name, foreign_key.target_index_name
        );
    };
    if target_index.fields.len() == 1 {
        format!(
            "{}.{}",
            foreign_key.target_table_name, target_index.fields[0].meta.name
        )
    } else {
        format!(
            "{}({})",
            foreign_key.target_table_name, foreign_key.target_index_name
        )
    }
}

fn relation_cardinality(foreign_key: &LinkedForeignKeyMeta<'_>) -> &'static str {
    if foreign_key.fields.len() == 1
        && matches!(foreign_key.fields[0].linked_field, LinkedField::List(_))
    {
        "list-to-many"
    } else {
        "many-to-one"
    }
}

fn validator_doc(scope: &str, validator: &ValidatorMeta) -> SchemaValidatorDoc {
    SchemaValidatorDoc {
        scope: scope.to_owned(),
        name: validator
            .name
            .clone()
            .unwrap_or_else(|| validator.kind.clone()),
        summary: validator_summary(validator),
        severity: format!("{:?}", validator.severity).to_ascii_lowercase(),
        description: validator.desc.clone().or_else(|| validator.message.clone()),
    }
}

fn validator_summary(validator: &ValidatorMeta) -> String {
    if validator.params.is_empty() {
        return validator.kind.clone();
    }
    let params = validator
        .params
        .iter()
        .map(|(key, value)| format!("{key}={}", toml_value_display(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{}({params})", validator.kind)
}

fn source_summary(table: &LinkedTableMeta<'_>) -> Option<String> {
    let sources = table
        .meta
        .source_paths()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if sources.is_empty() {
        None
    } else {
        Some(sources.join(", "))
    }
}

fn field_type_display(field: &LinkedFieldMeta<'_>) -> String {
    let display = linked_field_type_display(&field.linked_field);
    if matches!(
        field.linked_field.primitive_type(),
        Some(PrimitiveFieldType::Asset)
    ) {
        if let Some(asset_kind) = &field.meta.asset_kind {
            return format!("asset({asset_kind})");
        }
    }
    if matches!(
        field.linked_field.primitive_type(),
        Some(PrimitiveFieldType::Color)
    ) {
        return color_type_display(field);
    }
    if field
        .linked_field
        .primitive_type()
        .and_then(|primitive| primitive.vector_dimension())
        .is_some()
    {
        return vector_type_display(field, &display);
    }
    display
}

fn color_type_display(field: &LinkedFieldMeta<'_>) -> String {
    let format = field.meta.color_format.unwrap_or_default().as_str();
    let channel = field
        .meta
        .color_channel
        .map(|channel| channel.as_str())
        .unwrap_or("unspecified_channel");
    let color_space = field.meta.color_space.unwrap_or_default().as_str();
    let alpha = field.meta.color_alpha.unwrap_or_default().as_str();
    format!("color(format={format}, channel={channel}, space={color_space}, alpha={alpha})")
}

fn vector_type_display(field: &LinkedFieldMeta<'_>, base: &str) -> String {
    let mut parts = vec![format!(
        "format={}",
        field.meta.vector_format.unwrap_or_default().as_str()
    )];
    if let Some(components) = &field.meta.vector_components {
        parts.push(format!("components={}", components.join("/")));
    }
    if let Some(unit) = &field.meta.unit {
        parts.push(format!("unit={unit}"));
    }
    format!("{base}({})", parts.join(", "))
}

fn linked_field_type_display(field: &LinkedField<'_>) -> String {
    match field {
        LinkedField::Primitive(primitive) => primitive.as_str().to_owned(),
        LinkedField::Enum(enum_meta) => format!("enum({})", enum_meta.name),
        LinkedField::Struct(struct_type) => format!("struct({})", struct_type.meta.name),
        LinkedField::List(element) => format!("list<{}>", linked_field_type_display(element)),
    }
}

fn table_kind_display(kind: TableKind) -> &'static str {
    match kind {
        TableKind::Collection => "CollectionTable",
        TableKind::Object => "ObjectTable",
    }
}

fn index_signature(index: &crate::meta::LinkedIndexMeta<'_>) -> String {
    format!(
        "{}({})",
        index.meta.name,
        index
            .fields
            .iter()
            .map(|field| field.meta.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn export_visibility(export: bool, condition: &ExportCondition, currently_visible: bool) -> String {
    if !export {
        return "internal".to_owned();
    }
    if !currently_visible {
        return "hidden".to_owned();
    }
    condition_display(condition)
}

fn condition_display(condition: &ExportCondition) -> String {
    if condition.any.is_empty() && condition.all.is_empty() && condition.none.is_empty() {
        return "all".to_owned();
    }
    let mut parts = Vec::new();
    if !condition.any.is_empty() {
        parts.push(condition.any.join(","));
    }
    if !condition.all.is_empty() {
        parts.push(format!("all:{}", condition.all.join(",")));
    }
    if !condition.none.is_empty() {
        parts.push(format!("not:{}", condition.none.join(",")));
    }
    parts.join("; ")
}

fn asset_kind_rules(asset_kind: &AssetKindMeta) -> String {
    let mut parts = Vec::new();
    if !asset_kind.extensions.is_empty() {
        parts.push(format!("extensions {}", asset_kind.extensions.join("/")));
    }
    if !asset_kind.name_prefixes.is_empty() {
        parts.push(format!("prefix {}", asset_kind.name_prefixes.join("/")));
    }
    if !asset_kind.name_suffixes.is_empty() {
        parts.push(format!("suffix {}", asset_kind.name_suffixes.join("/")));
    }
    if asset_kind.require_existing == Some(true) {
        parts.push("require existing".to_owned());
    }
    if !asset_kind.allow_path_prefixes.is_empty() {
        parts.push(format!(
            "allow path {}",
            asset_kind.allow_path_prefixes.join("/")
        ));
    }
    if !asset_kind.deny_path_prefixes.is_empty() {
        parts.push(format!(
            "deny path {}",
            asset_kind.deny_path_prefixes.join("/")
        ));
    }
    if parts.is_empty() {
        "-".to_owned()
    } else {
        parts.join(", ")
    }
}

fn toml_value_display(value: &toml::Value) -> String {
    match value {
        toml::Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}
