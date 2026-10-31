// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use super::{
    AssetKindMeta, ColorFormat, FieldMeta, FragmentMeta, MetaDiagnostic, MetaDiagnosticCode,
    MetaModel, PrimitiveFieldType, TableKind, TableMeta, ValidatorMeta, ValueLayout,
    list_element_type_name, model::normalize_tag,
};

pub fn validate_meta_model(model: &MetaModel) -> Vec<MetaDiagnostic> {
    let mut diagnostics = Vec::new();
    validate_unique_names(model, &mut diagnostics);
    validate_asset_kinds(model, &mut diagnostics);
    validate_enums(model, &mut diagnostics);
    for struct_meta in &model.structs {
        validate_struct_meta(model, struct_meta, &mut diagnostics);
    }
    for table in &model.tables {
        validate_table_meta(model, table, &mut diagnostics);
    }
    diagnostics.sort();
    diagnostics
}

fn validate_struct_meta(
    model: &MetaModel,
    struct_meta: &super::StructMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let owner = format!("struct `{}`", struct_meta.name);
    validate_field_names(
        &owner,
        struct_meta.fields.iter().map(|field| field.name.as_str()),
        diagnostics,
    );

    for field in &struct_meta.fields {
        for validator in &field.validators {
            validate_validator_meta(
                &format!(
                    "struct `{}` field `{}` validator",
                    struct_meta.name, field.name
                ),
                validator,
                diagnostics,
            );
        }
        if !field.validators.is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "struct `{}` field `{}` declares validators, but struct child validators are not implemented yet",
                    struct_meta.name, field.name
                ),
            ));
        }
        validate_deprecated_field_export_flags(&owner, field, diagnostics);
        validate_unsupported_field_options(&owner, field, diagnostics);
        validate_asset_field_options(model, &owner, field, diagnostics);
        validate_builtin_value_field_options(&owner, field, diagnostics);
        validate_field_layout(model, &owner, field, diagnostics);
        validate_default_value(model, &owner, field, diagnostics);
    }
}

fn validate_unique_names(model: &MetaModel, diagnostics: &mut Vec<MetaDiagnostic>) {
    validate_names(
        model.tables.iter().map(|table| table.name.as_str()),
        MetaDiagnosticCode::DuplicateTableName,
        "table",
        diagnostics,
    );
    validate_names(
        model.enums.iter().map(|enum_meta| enum_meta.name.as_str()),
        MetaDiagnosticCode::DuplicateEnumName,
        "enum",
        diagnostics,
    );
    validate_names(
        model
            .structs
            .iter()
            .map(|struct_meta| struct_meta.name.as_str()),
        MetaDiagnosticCode::DuplicateStructName,
        "struct",
        diagnostics,
    );
    validate_names(
        model
            .asset_kinds
            .iter()
            .map(|asset_kind| asset_kind.name.as_str()),
        MetaDiagnosticCode::DuplicateAssetKindName,
        "asset kind",
        diagnostics,
    );
    validate_type_namespace_names(model, diagnostics);
}

fn validate_asset_kinds(model: &MetaModel, diagnostics: &mut Vec<MetaDiagnostic>) {
    for asset_kind in &model.asset_kinds {
        validate_asset_kind_meta(model, asset_kind, diagnostics);
    }
    validate_asset_kind_extends_cycles(model, diagnostics);
}

fn validate_asset_kind_meta(
    model: &MetaModel,
    asset_kind: &AssetKindMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let owner = format!("asset kind `{}`", asset_kind.name);
    if asset_kind.name.trim().is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidAssetKindDeclaration,
            "asset kind name must be non-empty",
        ));
    }
    if let Some(parent) = asset_kind.extends.as_deref() {
        if parent.trim().is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!("{owner} extends must be non-empty when declared"),
            ));
        } else if parent == asset_kind.name {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!("{owner} cannot extend itself"),
            ));
        } else if model.asset_kind(parent).is_none() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!("{owner} extends unknown asset kind `{parent}`"),
            ));
        }
    }
    for (field, values) in [
        ("search_paths", &asset_kind.search_paths),
        ("base_dirs", &asset_kind.base_dirs),
        ("extensions", &asset_kind.extensions),
        ("strip_wrappers", &asset_kind.strip_wrappers),
        ("require_sidecars", &asset_kind.require_sidecars),
        ("allow_path_prefixes", &asset_kind.allow_path_prefixes),
        ("deny_path_prefixes", &asset_kind.deny_path_prefixes),
        ("deny_when_tags", &asset_kind.deny_when_tags),
        ("name_prefixes", &asset_kind.name_prefixes),
        ("name_suffixes", &asset_kind.name_suffixes),
        ("allow_patterns", &asset_kind.allow_patterns),
        ("deny_patterns", &asset_kind.deny_patterns),
    ] {
        for value in values {
            if value.trim().is_empty() {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidAssetKindDeclaration,
                    format!("{owner} {field} contains an empty value"),
                ));
            }
        }
    }
    for tag in &asset_kind.deny_when_tags {
        if normalize_tag(tag.clone()).as_deref() != Some(tag.as_str()) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!("{owner} deny_when_tags value `{tag}` is invalid; tags must be non-empty and trimmed"),
            ));
        }
    }
    for pattern in asset_kind
        .allow_patterns
        .iter()
        .chain(asset_kind.deny_patterns.iter())
    {
        validate_regex_pattern(&owner, pattern, diagnostics);
    }
    let mut regex_rule_names = BTreeSet::new();
    for rule in &asset_kind.regex_rules {
        if rule.name.trim().is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!("{owner} regex rule name must be non-empty"),
            ));
        }
        if !regex_rule_names.insert(rule.name.as_str()) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!(
                    "{owner} regex rule `{}` is defined more than once",
                    rule.name
                ),
            ));
        }
        for tag in &rule.when_tags {
            if normalize_tag(tag.clone()).as_deref() != Some(tag.as_str()) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidAssetKindDeclaration,
                    format!("{owner} regex rule `{}` when_tags value `{tag}` is invalid; tags must be non-empty and trimmed", rule.name),
                ));
            }
        }
        validate_regex_pattern(
            &format!("{owner} regex rule `{}`", rule.name),
            &rule.pattern,
            diagnostics,
        );
    }
}

fn validate_asset_kind_extends_cycles(model: &MetaModel, diagnostics: &mut Vec<MetaDiagnostic>) {
    let parent_by_kind = model
        .asset_kinds
        .iter()
        .filter_map(|asset_kind| {
            asset_kind
                .extends
                .as_deref()
                .map(|parent| (asset_kind.name.as_str(), parent))
        })
        .collect::<BTreeMap<_, _>>();

    for asset_kind in &model.asset_kinds {
        let mut path = Vec::new();
        let mut visited = BTreeSet::new();
        let mut current = asset_kind.name.as_str();
        while let Some(parent) = parent_by_kind.get(current).copied() {
            if !visited.insert(current) {
                break;
            }
            path.push(current);
            if let Some(cycle_start) = path.iter().position(|kind| *kind == parent) {
                let mut cycle = path[cycle_start..].to_vec();
                cycle.push(parent);
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidAssetKindDeclaration,
                    format!("asset kind extends cycle detected: {}", cycle.join(" -> ")),
                ));
                break;
            }
            current = parent;
        }
    }
}

fn validate_regex_pattern(owner: &str, pattern: &str, diagnostics: &mut Vec<MetaDiagnostic>) {
    if let Err(error) = regex::Regex::new(pattern) {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidAssetKindDeclaration,
            format!("{owner} regex pattern `{pattern}` is invalid: {error}"),
        ));
    }
}

fn validate_type_namespace_names(model: &MetaModel, diagnostics: &mut Vec<MetaDiagnostic>) {
    let enum_names = model
        .enums
        .iter()
        .map(|enum_meta| enum_meta.name.as_str())
        .collect::<BTreeSet<_>>();
    for struct_meta in &model.structs {
        if enum_names.contains(struct_meta.name.as_str()) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::DuplicateTypeName,
                format!(
                    "type `{}` is defined as both enum and struct",
                    struct_meta.name
                ),
            ));
        }
    }

    for type_name in model
        .enums
        .iter()
        .map(|enum_meta| enum_meta.name.as_str())
        .chain(
            model
                .structs
                .iter()
                .map(|struct_meta| struct_meta.name.as_str()),
        )
    {
        if PrimitiveFieldType::parse(type_name).is_some() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::ReservedTypeName,
                format!("custom type `{type_name}` conflicts with a primitive field type"),
            ));
        }
        if matches!(type_name, "MetaModel" | "LinkedMetaModel") {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::ReservedTypeName,
                format!("custom type `{type_name}` conflicts with an internal meta model type"),
            ));
        }
    }
}

fn validate_names<'a>(
    names: impl Iterator<Item = &'a str>,
    code: MetaDiagnosticCode,
    kind: &str,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            diagnostics.push(MetaDiagnostic::new(
                code,
                format!("{} `{}` is defined more than once", kind, name),
            ));
        }
    }
}

fn validate_enums(model: &MetaModel, diagnostics: &mut Vec<MetaDiagnostic>) {
    for enum_meta in &model.enums {
        validate_names(
            enum_meta.values.iter().map(|value| value.name.as_str()),
            MetaDiagnosticCode::DuplicateEnumValueName,
            &format!("enum `{}` value", enum_meta.name),
            diagnostics,
        );
    }
}

fn validate_table_meta(
    model: &MetaModel,
    table: &TableMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    validate_source_declaration(table, diagnostics);
    validate_object_table_restrictions(table, diagnostics);
    validate_table_unsupported_options(table, diagnostics);
    validate_export_condition_tags(
        &format!("table `{}`", table.name),
        &table.export_when,
        diagnostics,
    );

    for validator in &table.validators {
        validate_validator_meta(
            &format!("table `{}` validator", table.name),
            validator,
            diagnostics,
        );
    }

    let field_names = validate_field_names(
        &format!("table `{}`", table.name),
        table.fields.iter().map(|field| field.name.as_str()),
        diagnostics,
    );
    validate_fragments(model, table, &field_names, diagnostics);

    for field in &table.fields {
        for validator in &field.validators {
            validate_validator_meta(
                &format!("table `{}` field `{}` validator", table.name, field.name),
                validator,
                diagnostics,
            );
        }
        validate_export_condition_tags(
            &format!("table `{}` field `{}`", table.name, field.name),
            &field.export_when,
            diagnostics,
        );
        validate_deprecated_field_export_flags(
            &format!("table `{}`", table.name),
            field,
            diagnostics,
        );
        validate_unsupported_field_options(&format!("table `{}`", table.name), field, diagnostics);
        validate_asset_field_options(
            model,
            &format!("table `{}`", table.name),
            field,
            diagnostics,
        );
        validate_builtin_value_field_options(
            &format!("table `{}`", table.name),
            field,
            diagnostics,
        );
        validate_field_layout(
            model,
            &format!("table `{}`", table.name),
            field,
            diagnostics,
        );
        validate_default_value(
            model,
            &format!("table `{}`", table.name),
            field,
            diagnostics,
        );
    }

    let mut index_names = BTreeSet::new();
    let mut primary_indexes = Vec::new();
    for index in &table.indexes {
        validate_export_condition_tags(
            &format!("table `{}` index `{}`", table.name, index.name),
            &index.export_when,
            diagnostics,
        );
        if !index_names.insert(index.name.as_str()) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::DuplicateIndexName,
                format!(
                    "table `{}` index `{}` is defined more than once",
                    table.name, index.name
                ),
            ));
        }

        if index.fields.is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::EmptyIndexFields,
                format!(
                    "table `{}` index `{}` must reference at least one field",
                    table.name, index.name
                ),
            ));
        }

        for field_name in &index.fields {
            if !field_names.contains(field_name.as_str()) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::MissingFieldReference,
                    format!(
                        "table `{}` index `{}` references unknown field `{}`",
                        table.name, index.name, field_name
                    ),
                ));
            }
        }

        if index.primary {
            primary_indexes.push(index.name.as_str());
            if !index.unique {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidPrimaryIndex,
                    format!(
                        "table `{}` index `{}` is primary but is not unique",
                        table.name, index.name
                    ),
                ));
            }
        }
    }

    validate_row_export_policy(table, &field_names, diagnostics);
    validate_primary_index(table, &primary_indexes, diagnostics);
    validate_foreign_keys(model, table, &field_names, diagnostics);
}

fn validate_foreign_keys(
    model: &MetaModel,
    table: &TableMeta,
    field_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let mut foreign_key_names = BTreeSet::new();
    let mut linked_field_names = BTreeSet::new();
    for foreign_key in table.resolved_foreign_keys() {
        let owner = format!("table `{}` foreign key `{}`", table.name, foreign_key.name);
        if foreign_key.name.trim().is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!("{owner} name must be non-empty"),
            ));
        }
        if !foreign_key_names.insert(foreign_key.name) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "table `{}` foreign key `{}` is defined more than once",
                    table.name, foreign_key.name
                ),
            ));
        }
        validate_export_condition_tags(&owner, foreign_key.export_when, diagnostics);

        if let Some(linked_field) = foreign_key.linked_field {
            if linked_field.trim().is_empty() {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!("{owner} linked_field must be non-empty"),
                ));
            }
            if !linked_field_names.insert(linked_field) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key linked_field `{}` is defined more than once",
                        table.name, linked_field
                    ),
                ));
            }
            if field_names.contains(linked_field) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "{owner} linked_field `{linked_field}` conflicts with source field of the same name"
                    ),
                ));
            }
            if table
                .fragments
                .iter()
                .any(|fragment| fragment.name == linked_field)
            {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "{owner} linked_field `{linked_field}` conflicts with fragment of the same name"
                    ),
                ));
            }
        }

        if foreign_key.fields.is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!("{owner} must reference at least one source field"),
            ));
        }

        let mut source_field_names = BTreeSet::new();
        for field_name in &foreign_key.fields {
            if field_name.contains('.') {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::UnsupportedFeature,
                    format!("{owner} references `{field_name}`, but fragment/nested field foreign keys are not supported"),
                ));
                continue;
            }
            if !source_field_names.insert(*field_name) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!("{owner} references source field `{field_name}` more than once"),
                ));
            }
            if !field_names.contains(field_name) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::MissingFieldReference,
                    format!("{owner} references unknown source field `{field_name}`"),
                ));
            }
        }

        if foreign_key.target_table.trim().is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!("{owner} target_table must be non-empty"),
            ));
            continue;
        }

        let Some(target_table) = model.table(foreign_key.target_table) else {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "{owner} references unknown target table `{}`",
                    foreign_key.target_table
                ),
            ));
            continue;
        };
        if target_table.kind != TableKind::Collection {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!("{owner} uses ObjectTable `{}` as target, but foreign key targets must be CollectionTable indexes", target_table.name),
            ));
        }

        let target_index = if let Some(target_index_name) = foreign_key.target_index {
            match target_table.index(target_index_name) {
                Some(index) => Some(index),
                None => {
                    diagnostics.push(MetaDiagnostic::new(
                        MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                        format!(
                            "{owner} references unknown target index `{}.{}`",
                            target_table.name, target_index_name
                        ),
                    ));
                    None
                }
            }
        } else {
            match target_table.primary_index() {
                Some(index) => Some(index),
                None => {
                    diagnostics.push(MetaDiagnostic::new(
                        MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                        format!(
                            "{owner} omits `target_index`, but target table `{}` has no primary index; configure a primary index or specify `target_index`",
                            target_table.name
                        ),
                    ));
                    None
                }
            }
        };

        let Some(target_index) = target_index else {
            continue;
        };
        if !(target_index.unique || target_index.primary) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "{owner} references target index `{}.{}` which is not unique",
                    target_table.name, target_index.name
                ),
            ));
        }
        if foreign_key.is_field_level() && target_index.fields.len() != 1 {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "{owner} references composite index `{}.{}`; field-level foreign key only supports single-field index, use [[tables.foreign_keys]] for composite keys",
                    target_table.name, target_index.name
                ),
            ));
        }
        if foreign_key.fields.len() != target_index.fields.len() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "{owner} has {} source fields but target index `{}.{}` has {} fields",
                    foreign_key.fields.len(),
                    target_table.name,
                    target_index.name,
                    target_index.fields.len()
                ),
            ));
            continue;
        }

        for (source_field_name, target_field_name) in foreign_key
            .fields
            .iter()
            .zip(target_index.fields.iter().map(String::as_str))
        {
            let Some(source_field) = table.field(source_field_name) else {
                continue;
            };
            let Some(target_field) = target_table.field(target_field_name) else {
                continue;
            };
            if !foreign_key_field_types_compatible(model, source_field, target_field) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "{owner} source field `{}` type `{}` is incompatible with target field `{}.{}` type `{}`",
                        source_field.name,
                        source_field.field_type,
                        target_table.name,
                        target_field.name,
                        target_field.field_type
                    ),
                ));
            }
        }
    }
}

fn validate_object_table_restrictions(table: &TableMeta, diagnostics: &mut Vec<MetaDiagnostic>) {
    if table.kind != TableKind::Object {
        return;
    }

    let owner = format!("table `{}`", table.name);
    if table.row_export.is_some() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} is an ObjectTable, but ObjectTable Step 1 does not support `row_export`"
            ),
        ));
    }
    if !table.fragments.is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!("{owner} is an ObjectTable, but ObjectTable Step 1 does not support fragments"),
        ));
    }
    if !table.indexes.is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!("{owner} is an ObjectTable, but ObjectTable Step 1 does not support indexes"),
        ));
    }
    if table.primary_index.is_some() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} is an ObjectTable, but ObjectTable Step 1 does not support `primary_index`"
            ),
        ));
    }
    if table.preserve_source_order {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!("{owner} is an ObjectTable, but ObjectTable Step 1 does not support `preserve_source_order`"),
        ));
    }
    if table.polymorphic {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} is an ObjectTable, but ObjectTable Step 1 does not support `polymorphic`"
            ),
        ));
    }

    for field in &table.fields {
        if field.unique {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} field `{}` is in an ObjectTable, but ObjectTable Step 1 does not support unique field indexes",
                    field.name
                ),
            ));
        }
    }
}

fn foreign_key_field_types_compatible(
    model: &MetaModel,
    source: &FieldMeta,
    target: &FieldMeta,
) -> bool {
    let source_primitive = PrimitiveFieldType::parse(source.foreign_key_scalar_type_name());
    match (source_primitive, target.primitive_type()) {
        (Some(source), Some(target)) => return source == target,
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    let source_type = source.foreign_key_scalar_type_name();
    let target_type = target.field_type.as_str();
    if model.enum_meta(source_type).is_some() || model.enum_meta(target_type).is_some() {
        return source_type == target_type
            && model.enum_meta(source_type).is_some()
            && model.enum_meta(target_type).is_some();
    }
    false
}

fn validate_field_layout(
    model: &MetaModel,
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let is_struct = model.struct_meta(&field.field_type).is_some();
    let list_element_type = list_element_type_name(&field.field_type);
    if let Some(element_type) = list_element_type
        && (list_element_type_name(element_type).is_some()
            || model.struct_meta(element_type).is_some())
    {
        diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{} field `{}` uses unsupported list element type `{}`; list fields currently support only primitive or enum elements",
                    owner, field.name, element_type
                ),
            ));
    }
    let is_list = list_element_type.is_some();
    match (is_struct, is_list, field.layout) {
        (true, _, None) => {}
        (true, _, Some(_)) => diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{} field `{}` uses struct type `{}` with explicit layout; struct source layout is decided by CSV headers or object keys, so remove `layout`",
                owner, field.name, field.field_type
            ),
        )),
        (false, true, None | Some(ValueLayout::JsonCell)) => {}
        (false, true, Some(ValueLayout::ScalarCell | ValueLayout::ExpandedValue)) => {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{} field `{}` uses unsupported layout; list fields must use JSON cell values in this phase",
                    owner, field.name
                ),
            ));
        }
        (false, false, Some(ValueLayout::JsonCell | ValueLayout::ExpandedValue)) => {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{} field `{}` uses layout `{:?}`, but non-struct fields only support scalar_cell in this phase",
                    owner,
                    field.name,
                    field.layout.expect("layout exists")
                ),
            ));
        }
        (false, false, None | Some(ValueLayout::ScalarCell)) => {}
    }

    if is_struct && field.default.is_some() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{} field `{}` uses struct type `{}` with a default value, but whole-struct defaults are not implemented yet",
                owner, field.name, field.field_type
            ),
        ));
    }
}

fn validate_fragments(
    model: &MetaModel,
    table: &TableMeta,
    field_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let owner = format!("table `{}`", table.name);
    let fragment_names = validate_field_names(
        &format!("{owner} fragment"),
        table
            .fragments
            .iter()
            .map(|fragment| fragment.name.as_str()),
        diagnostics,
    );

    if !table.fragments.is_empty() {
        if table.kind != TableKind::Collection {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!("{owner} declares fragments, but fragments are only supported on collection tables"),
            ));
        }
        if !table.polymorphic {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!("{owner} declares fragments, but `polymorphic = true` is required"),
            ));
        }
    }

    for fragment in &table.fragments {
        validate_fragment_meta(model, table, fragment, field_names, diagnostics);
    }

    for source in table.sources.iter().chain(table.override_sources.iter()) {
        if !source.fragments.is_empty() {
            if table.kind != TableKind::Collection || !table.polymorphic {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::UnsupportedFeature,
                    format!(
                        "table `{}` source `{}` declares fragments, but fragments require a polymorphic collection table",
                        table.name, source.path
                    ),
                ));
            }
            for fragment_name in &source.fragments {
                if !fragment_names.contains(fragment_name.as_str()) {
                    diagnostics.push(MetaDiagnostic::new(
                        MetaDiagnosticCode::MissingFieldReference,
                        format!(
                            "table `{}` source `{}` references unknown fragment `{}`",
                            table.name, source.path, fragment_name
                        ),
                    ));
                }
            }
        }
    }
}

fn validate_fragment_meta(
    model: &MetaModel,
    table: &TableMeta,
    fragment: &FragmentMeta,
    field_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let owner = format!("table `{}` fragment `{}`", table.name, fragment.name);
    if field_names.contains(fragment.name.as_str()) {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::DuplicateFieldName,
            format!(
                "table `{}` fragment `{}` conflicts with a common field of the same name",
                table.name, fragment.name
            ),
        ));
    }
    validate_export_condition_tags(&owner, &fragment.export_when, diagnostics);
    if fragment.type_name.is_some() && !fragment.fields.is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{} declares both `type` and inline fields; fragment payload must use exactly one form",
                owner
            ),
        ));
    }
    if let Some(type_name) = fragment.type_name.as_deref()
        && model.struct_meta(type_name).is_none()
    {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnknownFieldType,
            format!("{} references unknown struct type `{}`", owner, type_name),
        ));
    }
    validate_field_names(
        &owner,
        fragment.fields.iter().map(|field| field.name.as_str()),
        diagnostics,
    );
    for field in &fragment.fields {
        if !field.validators.is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{} field `{}` declares validators, but fragment field validators are not implemented yet",
                    owner, field.name
                ),
            ));
        }
        for validator in &field.validators {
            validate_validator_meta(
                &format!("{} field `{}` validator", owner, field.name),
                validator,
                diagnostics,
            );
        }
        validate_export_condition_tags(
            &format!("{} field `{}`", owner, field.name),
            &field.export_when,
            diagnostics,
        );
        validate_deprecated_field_export_flags(&owner, field, diagnostics);
        validate_unsupported_field_options(&owner, field, diagnostics);
        validate_asset_field_options(model, &owner, field, diagnostics);
        validate_builtin_value_field_options(&owner, field, diagnostics);
        validate_field_layout(model, &owner, field, diagnostics);
        validate_default_value(model, &owner, field, diagnostics);
    }
}

fn validate_validator_meta(
    owner: &str,
    validator: &ValidatorMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    if validator.kind.trim().is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidValidatorConfig,
            format!("{owner} kind must be non-empty"),
        ));
    }

    validate_export_condition_tags(owner, &validator.export_when, diagnostics);
}

fn validate_export_condition_tags(
    owner: &str,
    condition: &super::ExportCondition,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    for tag in condition
        .any
        .iter()
        .chain(condition.all.iter())
        .chain(condition.none.iter())
    {
        if normalize_tag(tag.clone()).as_deref() != Some(tag.as_str()) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} export_when tag `{tag}` is invalid; tags must be non-empty and trimmed"
                ),
            ));
        }
    }
}

fn validate_deprecated_field_export_flags(
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    for deprecated in [
        ("tool_only", field.tool_only),
        ("validator_only", field.validator_only),
        ("filter_field", field.filter_field),
    ] {
        if deprecated.1.is_some() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} field `{}` uses deprecated `{}`; use `export = false`, `export_when`, or `row_export` instead",
                    field.name, deprecated.0
                ),
            ));
        }
    }
}

fn validate_table_unsupported_options(table: &TableMeta, diagnostics: &mut Vec<MetaDiagnostic>) {
    let _ = (table, diagnostics);
}

fn validate_unsupported_field_options(
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    if field.field_number.is_some() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} field `{}` uses `field_number`, but field numbers are reserved for future schema/codegen formats and are not implemented yet",
                field.name
            ),
        ));
    }
}

fn validate_asset_field_options(
    model: &MetaModel,
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let Some(asset_kind) = field.asset_kind.as_deref() else {
        if field.is_asset_scalar() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidAssetKindDeclaration,
                format!(
                    "{owner} field `{}` uses type `asset` but does not declare `asset_kind`",
                    field.name
                ),
            ));
        }
        return;
    };

    if asset_kind.trim().is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidAssetKindDeclaration,
            format!(
                "{owner} field `{}` asset_kind must be non-empty",
                field.name
            ),
        ));
    }
    if !field.is_asset_scalar() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidAssetKindDeclaration,
            format!(
                "{owner} field `{}` declares `asset_kind`, but only type `asset` fields may use it",
                field.name
            ),
        ));
        return;
    }
    if model.asset_kind(asset_kind).is_none() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidAssetKindDeclaration,
            format!(
                "{owner} field `{}` references unknown asset_kind `{asset_kind}`",
                field.name
            ),
        ));
    }
}

fn validate_builtin_value_field_options(
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    if field.has_color_options() && !field.is_color_scalar() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} field `{}` declares color options, but only type `color` fields may use them",
                field.name
            ),
        ));
    }
    if field.has_vector_options() && field.vector_dimension().is_none() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{owner} field `{}` declares vector options, but only vector2/vector3/vector4/vector*_int fields may use them",
                field.name
            ),
        ));
    }

    if field.is_color_scalar() {
        if field.color_channel.is_none() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} field `{}` uses type `color` but does not declare `color_channel`; set it to `byte` or `normalized`",
                    field.name
                ),
            ));
        }
        if matches!(
            field.color_format,
            Some(ColorFormat::HexRgb | ColorFormat::RgbTuple)
        ) && field.color_alpha.is_none()
        {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} field `{}` uses RGB color format without alpha; set `alpha = \"default_opaque\"` or use an RGBA format",
                    field.name
                ),
            ));
        }
    }

    if let Some(dimension) = field.vector_dimension()
        && let Some(components) = &field.vector_components
    {
        if components.len() != dimension {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{owner} field `{}` declares {} vector components but type `{}` requires {}",
                    field.name,
                    components.len(),
                    field.field_type,
                    dimension
                ),
            ));
        }
        let mut seen = BTreeSet::new();
        for component in components {
            if component.trim().is_empty() || !seen.insert(component.as_str()) {
                diagnostics.push(MetaDiagnostic::new(
                        MetaDiagnosticCode::UnsupportedFeature,
                        format!(
                            "{owner} field `{}` declares invalid vector_components; component names must be non-empty and unique",
                            field.name
                        ),
                    ));
                break;
            }
        }
    }
}

fn validate_row_export_policy(
    table: &TableMeta,
    field_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let Some(row_export) = &table.row_export else {
        return;
    };

    for field_name in [
        row_export.required_tags_field.as_deref(),
        row_export.excluded_tags_field.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !field_names.contains(field_name) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::MissingFieldReference,
                format!(
                    "table `{}` row_export references unknown field `{}`",
                    table.name, field_name
                ),
            ));
        }
    }
}

fn validate_source_declaration(table: &TableMeta, diagnostics: &mut Vec<MetaDiagnostic>) {
    if table.source.is_some() && !table.sources.is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidSourceDeclaration,
            format!(
                "table `{}` declares both legacy `source` and `sources`; use exactly one source form",
                table.name
            ),
        ));
    }

    if table.kind == TableKind::Collection
        && table.sources.len() > 1
        && !table.is_variant_source_mode()
    {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "table `{}` declares multiple sources, but only polymorphic collection tables with fragments support variant source mode",
                table.name
            ),
        ));
    }

    for source in &table.sources {
        if source.export_when != super::ExportCondition::default() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "table `{}` base source `{}` declares `export_when`, but source-level target filtering is currently supported only for `override_sources`",
                    table.name, source.path
                ),
            ));
        }
    }

    if !table.override_sources.is_empty()
        && table.kind == TableKind::Collection
        && table.primary_index().is_none()
    {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidSourceDeclaration,
            format!(
                "table `{}` declares override_sources, but collection table overrides require a primary index",
                table.name
            ),
        ));
    }

    for source in table
        .source
        .iter()
        .chain(table.sources.iter())
        .chain(table.override_sources.iter())
    {
        if source.provider != "csv" {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidSourceDeclaration,
                format!(
                    "table `{}` source `{}` uses unsupported provider `{}`; only `csv` is supported",
                    table.name, source.path, source.provider
                ),
            ));
        }
        validate_source_row_layout(table, source, diagnostics);
    }

    for source in &table.override_sources {
        validate_export_condition_tags(
            &format!("table `{}` override source `{}`", table.name, source.path),
            &source.export_when,
            diagnostics,
        );
    }
}

fn validate_source_row_layout(
    table: &TableMeta,
    source: &super::SourceRef,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    if source.header_row == 0 {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidSourceDeclaration,
            format!(
                "table `{}` source `{}` declares `header_row = 0`, but source row positions are 1-based",
                table.name, source.path
            ),
        ));
    }
    if source.data_start_row == 0 {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidSourceDeclaration,
            format!(
                "table `{}` source `{}` declares `data_start_row = 0`, but source row positions are 1-based",
                table.name, source.path
            ),
        ));
    }
    if source.header_row > 0
        && source.data_start_row > 0
        && source.data_start_row <= source.header_row
    {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidSourceDeclaration,
            format!(
                "table `{}` source `{}` declares `data_start_row = {}` before or at `header_row = {}`; data rows must start after the header row",
                table.name, source.path, source.data_start_row, source.header_row
            ),
        ));
    }
}

fn validate_default_value(
    model: &MetaModel,
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let default = if let Some(default) = &field.default {
        default
    } else {
        return;
    };

    let valid = if let Some(element_type) = list_element_type_name(&field.field_type) {
        default.as_array().is_some_and(|values| {
            values
                .iter()
                .all(|value| default_value_matches_type(model, element_type, value))
        })
    } else {
        match PrimitiveFieldType::parse(&field.field_type) {
            Some(PrimitiveFieldType::Int) => default.as_integer().is_some(),
            Some(PrimitiveFieldType::Float) => {
                default.as_integer().is_some() || default.as_float().is_some_and(f64::is_finite)
            }
            Some(PrimitiveFieldType::Bool) => default.as_bool().is_some(),
            Some(
                PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset,
            ) => default.as_str().is_some(),
            Some(primitive) if primitive.is_builtin_value_type() => default.as_str().is_some(),
            Some(_) => false,
            None => {
                if model.struct_meta(&field.field_type).is_some() {
                    return;
                }
                model
                    .enum_meta(&field.field_type)
                    .and_then(|enum_meta| {
                        default.as_str().map(|default_value| {
                            enum_meta
                                .values
                                .iter()
                                .any(|enum_value| enum_value.name == default_value)
                        })
                    })
                    .unwrap_or(false)
            }
        }
    };

    if !valid {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidDefaultValue,
            format!(
                "{} field `{}` default value does not match type `{}`",
                owner, field.name, field.field_type
            ),
        ));
    }
}

fn default_value_matches_type(model: &MetaModel, field_type: &str, default: &toml::Value) -> bool {
    match PrimitiveFieldType::parse(field_type) {
        Some(PrimitiveFieldType::Int) => default.as_integer().is_some(),
        Some(PrimitiveFieldType::Float) => {
            default.as_integer().is_some() || default.as_float().is_some_and(f64::is_finite)
        }
        Some(PrimitiveFieldType::Bool) => default.as_bool().is_some(),
        Some(PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset) => {
            default.as_str().is_some()
        }
        Some(primitive) if primitive.is_builtin_value_type() => default.as_str().is_some(),
        Some(_) => false,
        None => model.enum_meta(field_type).is_some_and(|enum_meta| {
            default.as_str().is_some_and(|default_value| {
                enum_meta
                    .values
                    .iter()
                    .any(|enum_value| enum_value.name == default_value)
            })
        }),
    }
}

fn validate_field_names<'a>(
    owner: &str,
    names: impl Iterator<Item = &'a str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::DuplicateFieldName,
                format!("{} field `{}` is defined more than once", owner, name),
            ));
        }
        if name.contains(['.', '[', ']']) {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "{} field `{}` contains reserved field path characters `.`, `[` or `]`",
                    owner, name
                ),
            ));
        }
    }
    seen
}

fn validate_primary_index(
    table: &TableMeta,
    primary_indexes: &[&str],
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    if primary_indexes.len() > 1 {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::PrimaryIndexMismatch,
            format!(
                "table `{}` declares multiple primary indexes: {}",
                table.name,
                primary_indexes.join(", ")
            ),
        ));
    }

    let primary_index_name = match table.primary_index.as_deref() {
        Some(primary_index_name) => primary_index_name,
        None => return,
    };

    let primary_index = if let Some(primary_index) = table.index(primary_index_name) {
        primary_index
    } else {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidPrimaryIndex,
            format!(
                "table `{}` primary_index `{}` does not exist",
                table.name, primary_index_name
            ),
        ));
        return;
    };

    if !primary_index.unique {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidPrimaryIndex,
            format!(
                "table `{}` primary_index `{}` is not unique",
                table.name, primary_index_name
            ),
        ));
    }

    if let Some(marked_primary) = primary_indexes.first()
        && *marked_primary != primary_index_name
    {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::PrimaryIndexMismatch,
            format!(
                "table `{}` primary_index `{}` conflicts with primary index `{}`",
                table.name, primary_index_name, marked_primary
            ),
        ));
    }
}
