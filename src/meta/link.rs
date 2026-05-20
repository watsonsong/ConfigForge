use std::collections::HashMap;

use super::{
    FieldMeta, LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta,
    LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType, LinkedTableMeta,
    MetaDiagnostic, MetaDiagnosticCode, MetaModel, PrimitiveFieldType,
};

pub fn link_meta_model(model: &MetaModel) -> (Option<LinkedMetaModel<'_>>, Vec<MetaDiagnostic>) {
    let mut diagnostics = Vec::new();
    let enum_index = model
        .enums
        .iter()
        .map(|enum_meta| (enum_meta.name.as_str(), enum_meta))
        .collect::<HashMap<_, _>>();
    let struct_index = model
        .structs
        .iter()
        .map(|struct_meta| (struct_meta.name.as_str(), struct_meta))
        .collect::<HashMap<_, _>>();

    let mut linked_tables = Vec::new();
    let raw_table_index = model
        .tables
        .iter()
        .map(|table| (table.name.as_str(), table))
        .collect::<HashMap<_, _>>();
    for table in &model.tables {
        let linked_fields = link_fields(
            &enum_index,
            &struct_index,
            &table.fields,
            &mut Vec::new(),
            &mut diagnostics,
        );
        let linked_fragments = table
            .fragments
            .iter()
            .map(|fragment| {
                let linked_fields = link_fields(
                    &enum_index,
                    &struct_index,
                    &fragment.fields,
                    &mut Vec::new(),
                    &mut diagnostics,
                );
                LinkedFragmentMeta::new(fragment, linked_fields)
            })
            .collect::<Vec<_>>();
        let linked_indexes = link_indexes(table, &linked_fields);
        let linked_foreign_keys =
            link_foreign_keys(table, &raw_table_index, &linked_fields, &mut diagnostics);
        linked_tables.push(LinkedTableMeta::new(
            table,
            linked_fields,
            linked_fragments,
            linked_indexes,
            linked_foreign_keys,
        ));
    }

    let linked_structs = model
        .structs
        .iter()
        .map(|struct_meta| {
            let linked_fields = link_fields(
                &enum_index,
                &struct_index,
                &struct_meta.fields,
                &mut vec![struct_meta.name.as_str()],
                &mut diagnostics,
            );
            LinkedStructMeta::new(struct_meta, linked_fields)
        })
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        (
            Some(LinkedMetaModel::new(model, linked_tables, linked_structs)),
            diagnostics,
        )
    } else {
        (None, diagnostics)
    }
}

impl MetaModel {
    pub fn link(&self) -> std::result::Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>> {
        let (linked_model, diagnostics) = link_meta_model(self);
        if diagnostics.is_empty() {
            Ok(linked_model.expect("linked model should exist when diagnostics are empty"))
        } else {
            Err(diagnostics)
        }
    }
}

fn link_fields<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    fields: &'meta [FieldMeta],
    struct_stack: &mut Vec<&'meta str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Vec<LinkedFieldMeta<'meta>> {
    fields
        .iter()
        .filter_map(|field| {
            link_field_type(enum_index, struct_index, field, struct_stack, diagnostics)
        })
        .collect()
}

fn link_indexes<'meta>(
    table: &'meta super::TableMeta,
    linked_fields: &[LinkedFieldMeta<'meta>],
) -> Vec<LinkedIndexMeta<'meta>> {
    let field_index = linked_fields
        .iter()
        .map(|field| (field.meta.name.as_str(), field))
        .collect::<HashMap<_, _>>();
    table
        .indexes
        .iter()
        .map(|index| LinkedIndexMeta {
            meta: index,
            fields: index
                .fields
                .iter()
                .filter_map(|field_name| field_index.get(field_name.as_str()).cloned().cloned())
                .collect(),
        })
        .collect()
}

fn link_foreign_keys<'meta>(
    table: &'meta super::TableMeta,
    table_index: &HashMap<&'meta str, &'meta super::TableMeta>,
    linked_fields: &[LinkedFieldMeta<'meta>],
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Vec<LinkedForeignKeyMeta<'meta>> {
    let field_index = linked_fields
        .iter()
        .map(|field| (field.meta.name.as_str(), field))
        .collect::<HashMap<_, _>>();
    let mut linked_foreign_keys = Vec::new();
    for foreign_key in table.resolved_foreign_keys() {
        let mut failed = false;
        let fields = foreign_key
            .fields
            .iter()
            .filter_map(|field_name| {
                field_index.get(field_name).cloned().cloned().or_else(|| {
                    failed = true;
                    diagnostics.push(MetaDiagnostic::new(
                        MetaDiagnosticCode::MissingFieldReference,
                        format!(
                            "table `{}` foreign key `{}` references unknown source field `{}`",
                            table.name, foreign_key.name, field_name
                        ),
                    ));
                    None
                })
            })
            .collect::<Vec<_>>();

        let Some(target_table) = table_index.get(foreign_key.target_table).copied() else {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                format!(
                    "table `{}` foreign key `{}` references unknown target table `{}`",
                    table.name, foreign_key.name, foreign_key.target_table
                ),
            ));
            continue;
        };
        let target_index = if let Some(target_index_name) = foreign_key.target_index {
            target_table.index(target_index_name).or_else(|| {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` references unknown target index `{}.{}`",
                        table.name, foreign_key.name, target_table.name, target_index_name
                    ),
                ));
                failed = true;
                None
            })
        } else {
            target_table.primary_index().or_else(|| {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` omits `target_index`, but target table `{}` has no primary index",
                        table.name, foreign_key.name, target_table.name
                    ),
                ));
                failed = true;
                None
            })
        };

        if failed {
            continue;
        }
        if let Some(target_index) = target_index {
            linked_foreign_keys.push(LinkedForeignKeyMeta {
                name: foreign_key.name,
                desc: foreign_key.desc,
                export: foreign_key.export,
                export_when: foreign_key.export_when,
                allow_empty: foreign_key.allow_empty,
                declared_at: foreign_key.declared_at,
                fields,
                target_table_name: target_table.name.as_str(),
                target_index_name: target_index.name.as_str(),
            });
        }
    }
    linked_foreign_keys
}

fn link_field_type<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    field: &'meta FieldMeta,
    struct_stack: &mut Vec<&'meta str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Option<LinkedFieldMeta<'meta>> {
    let linked_field = if let Some(primitive) = PrimitiveFieldType::parse(&field.field_type) {
        LinkedField::Primitive(primitive)
    } else if let Some(enum_meta) = enum_index.get(field.field_type.as_str()) {
        LinkedField::Enum(enum_meta)
    } else if let Some(struct_meta) = struct_index.get(field.field_type.as_str()) {
        let linked_struct = link_struct_type(
            enum_index,
            struct_index,
            struct_meta,
            struct_stack,
            diagnostics,
        )?;
        LinkedField::Struct(linked_struct)
    } else {
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnknownFieldType,
            format!(
                "field `{}` references unknown type `{}`",
                field.name, field.field_type
            ),
        ));
        return None;
    };

    Some(LinkedFieldMeta {
        meta: field,
        linked_field,
    })
}

fn link_struct_type<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    struct_meta: &'meta super::StructMeta,
    struct_stack: &mut Vec<&'meta str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Option<LinkedStructType<'meta>> {
    if struct_stack.contains(&struct_meta.name.as_str()) {
        let mut cycle_path = struct_stack.clone();
        cycle_path.push(struct_meta.name.as_str());
        diagnostics.push(MetaDiagnostic::new(
            MetaDiagnosticCode::UnsupportedFeature,
            format!(
                "struct type cycle `{}` is not supported by linked meta",
                cycle_path.join(" -> ")
            ),
        ));
        return None;
    }

    struct_stack.push(struct_meta.name.as_str());
    let fields = link_fields(
        enum_index,
        struct_index,
        &struct_meta.fields,
        struct_stack,
        diagnostics,
    );
    struct_stack.pop();

    Some(LinkedStructType {
        meta: struct_meta,
        fields,
    })
}
