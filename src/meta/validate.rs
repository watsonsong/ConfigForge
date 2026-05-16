use std::collections::BTreeSet;

use super::{MetaDiagnostic, MetaDiagnosticCode, MetaModel, TableMeta};

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
        validate_table_meta(table, &mut diagnostics);
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

fn validate_table_meta(table: &TableMeta, diagnostics: &mut Vec<MetaDiagnostic>) {
    let field_names = validate_field_names(
        &format!("table `{}`", table.name),
        table.fields.iter().map(|field| field.name.as_str()),
        diagnostics,
    );

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

    let primary_index = match table.index(primary_index_name) {
        Some(primary_index) => primary_index,
        None => {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidPrimaryIndex,
                format!(
                    "table `{}` primary_index `{}` does not exist",
                    table.name, primary_index_name
                ),
            ));
            return;
        }
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
