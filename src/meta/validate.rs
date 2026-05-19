use std::collections::BTreeSet;

use super::{
    model::normalize_tag, FieldMeta, FragmentMeta, MetaDiagnostic, MetaDiagnosticCode, MetaModel,
    PrimitiveFieldType, TableKind, TableMeta, ValidatorMeta, ValueLayout,
};

pub fn validate_meta_model(model: &MetaModel) -> Vec<MetaDiagnostic> {
    let mut diagnostics = Vec::new();
    validate_unique_names(model, &mut diagnostics);
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
    validate_type_namespace_names(model, diagnostics);
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
}

fn validate_field_layout(
    model: &MetaModel,
    owner: &str,
    field: &FieldMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) {
    let is_struct = model.struct_meta(&field.field_type).is_some();
    match (is_struct, field.layout) {
        (true, None | Some(ValueLayout::JsonCell)) => {}
        (true, Some(ValueLayout::ScalarCell)) => diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{} field `{}` uses struct type `{}` with scalar_cell layout; struct fields only support json_cell in this phase",
                owner, field.name, field.field_type
            ),
        )),
        (true, Some(ValueLayout::ExpandedValue)) => diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "{} field `{}` uses struct type `{}` with expanded_value layout, but expanded struct values are not implemented yet",
                owner, field.name, field.field_type
            ),
        )),
        (false, Some(ValueLayout::JsonCell | ValueLayout::ExpandedValue)) => {
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
        (false, None | Some(ValueLayout::ScalarCell)) => {}
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

    if table.sources.len() > 1 && !is_variant_source_mode(table) {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "table `{}` declares multiple sources, but only polymorphic collection tables with fragments support variant source mode",
                table.name
            ),
        ));
    }

    if !table.override_sources.is_empty() {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "table `{}` declares override_sources, but source override is not implemented yet",
                table.name
            ),
        ));
    }

    for source in table.sources.iter().chain(table.override_sources.iter()) {
        if source.provider != "csv" {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidSourceDeclaration,
                format!(
                    "table `{}` source `{}` uses unsupported provider `{}`; only `csv` is supported",
                    table.name, source.path, source.provider
                ),
            ));
        }
    }
}

fn is_variant_source_mode(table: &TableMeta) -> bool {
    table.kind == TableKind::Collection
        && table.polymorphic
        && (!table.fragments.is_empty()
            || table
                .sources
                .iter()
                .any(|source| !source.fragments.is_empty()))
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

    let valid = match PrimitiveFieldType::parse(&field.field_type) {
        Some(PrimitiveFieldType::Int) => default.as_integer().is_some(),
        Some(PrimitiveFieldType::Float) => {
            default.as_float().is_some() || default.as_integer().is_some()
        }
        Some(PrimitiveFieldType::Bool) => default.as_bool().is_some(),
        Some(PrimitiveFieldType::String | PrimitiveFieldType::Text | PrimitiveFieldType::Asset) => {
            default.as_str().is_some()
        }
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

    if let Some(marked_primary) = primary_indexes.first() {
        if *marked_primary != primary_index_name {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::PrimaryIndexMismatch,
                format!(
                    "table `{}` primary_index `{}` conflicts with primary index `{}`",
                    table.name, primary_index_name, marked_primary
                ),
            ));
        }
    }
}
