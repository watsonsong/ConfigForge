use std::collections::BTreeSet;

use super::{MetaDiagnostic, MetaDiagnosticCode, MetaModel, PrimitiveFieldType, TableMeta};

pub fn validate_meta_model(model: &MetaModel) -> Vec<MetaDiagnostic> {
    let mut diagnostics = Vec::new();
    validate_unique_names(model, &mut diagnostics);
    validate_enums(model, &mut diagnostics);
    for struct_meta in &model.structs {
        validate_field_names(
            &format!("struct `{}`", struct_meta.name),
            struct_meta.fields.iter().map(|field| field.name.as_str()),
            &mut diagnostics,
        );
    }
    for table in &model.tables {
        validate_table_meta(model, table, &mut diagnostics);
    }
    diagnostics.sort();
    diagnostics
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

    let field_names = validate_field_names(
        &format!("table `{}`", table.name),
        table.fields.iter().map(|field| field.name.as_str()),
        diagnostics,
    );

    for field in &table.fields {
        validate_default_value(model, table, field, diagnostics);
    }

    let mut index_names = BTreeSet::new();
    let mut primary_indexes = Vec::new();
    for index in &table.indexes {
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

    validate_primary_index(table, &primary_indexes, diagnostics);
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

    if table.sources.len() > 1 {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "table `{}` declares multiple sources, but multi-source merge is not implemented yet",
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

        if !source.fragments.is_empty() {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnsupportedFeature,
                format!(
                    "table `{}` source `{}` declares fragments, but polymorphic fragments are not implemented yet",
                    table.name, source.path
                ),
            ));
        }
    }
}

fn validate_default_value(
    model: &MetaModel,
    table: &TableMeta,
    field: &super::FieldMeta,
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
        None => model
            .enum_meta(&field.field_type)
            .and_then(|enum_meta| {
                default.as_str().map(|default_value| {
                    enum_meta
                        .values
                        .iter()
                        .any(|enum_value| enum_value.name == default_value)
                })
            })
            .unwrap_or(false),
    };

    if !valid {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::InvalidDefaultValue,
            format!(
                "table `{}` field `{}` default value does not match type `{}`",
                table.name, field.name, field.field_type
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
