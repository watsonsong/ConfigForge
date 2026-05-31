// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use super::{
    FieldMeta, LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta, LinkedFragmentMeta,
    LinkedFragmentPayload, LinkedIndexMeta, LinkedMetaModel, LinkedStructMeta, LinkedStructType,
    LinkedTableMeta, MetaDiagnostic, MetaDiagnosticCode, MetaModel, PrimitiveFieldType,
    list_element_type_name, validate::validate_meta_model,
};

/// Builds a borrowed linked view of a validated raw model.
///
/// Callers must only consume the returned linked model when diagnostics are empty.
/// Link diagnostics intentionally reject incomplete intermediate shapes so later
/// validation, export, and codegen stages can rely on linked references.
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
                let linked_payload =
                    link_fragment_payload(&enum_index, &struct_index, fragment, &mut diagnostics);
                LinkedFragmentMeta::new(fragment, linked_payload.fields, linked_payload.payload)
            })
            .collect::<Vec<_>>();
        let linked_indexes = link_indexes(table, &linked_fields, &mut diagnostics);
        let linked_foreign_keys = link_foreign_keys(
            table,
            &raw_table_index,
            &enum_index,
            &linked_fields,
            &mut diagnostics,
        );
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
    /// Builds a borrowed linked view after running both link-time and structural validation.
    ///
    /// Prefer this method for external callers. Use [`link_meta_model`] only when the
    /// caller intentionally needs raw link diagnostics before structural validation.
    pub fn link(&self) -> std::result::Result<LinkedMetaModel<'_>, Vec<MetaDiagnostic>> {
        let (linked_model, mut diagnostics) = link_meta_model(self);
        diagnostics.extend(validate_meta_model(self));
        diagnostics.sort();
        diagnostics.dedup();
        if diagnostics.is_empty() {
            Ok(linked_model.expect("linked model should exist when diagnostics are empty"))
        } else {
            Err(diagnostics)
        }
    }
}

struct LinkedFragmentPayloadResult<'meta> {
    fields: Vec<LinkedFieldMeta<'meta>>,
    payload: LinkedFragmentPayload<'meta>,
}

fn link_fragment_payload<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    fragment: &'meta super::FragmentMeta,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> LinkedFragmentPayloadResult<'meta> {
    if let Some(type_name) = fragment.type_name.as_deref() {
        let Some(struct_meta) = struct_index.get(type_name).copied() else {
            diagnostics.push(MetaDiagnostic::new(
                MetaDiagnosticCode::UnknownFieldType,
                format!(
                    "fragment `{}` references unknown struct type `{}`",
                    fragment.name, type_name
                ),
            ));
            return LinkedFragmentPayloadResult {
                fields: Vec::new(),
                payload: LinkedFragmentPayload::InlineFields,
            };
        };
        let Some(struct_type) = link_struct_type(
            enum_index,
            struct_index,
            struct_meta,
            &mut Vec::new(),
            diagnostics,
        ) else {
            return LinkedFragmentPayloadResult {
                fields: Vec::new(),
                payload: LinkedFragmentPayload::InlineFields,
            };
        };
        let fields = struct_type.fields.clone();
        return LinkedFragmentPayloadResult {
            fields,
            payload: LinkedFragmentPayload::Struct(struct_type),
        };
    }

    let fields = link_fields(
        enum_index,
        struct_index,
        &fragment.fields,
        &mut Vec::new(),
        diagnostics,
    );
    LinkedFragmentPayloadResult {
        fields,
        payload: LinkedFragmentPayload::InlineFields,
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
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Vec<LinkedIndexMeta<'meta>> {
    let field_index = linked_fields
        .iter()
        .map(|field| (field.meta.name.as_str(), field))
        .collect::<HashMap<_, _>>();
    table
        .indexes
        .iter()
        .filter_map(|index| {
            let mut missing_field = false;
            let fields = index
                .fields
                .iter()
                .filter_map(|field_name| {
                    field_index
                        .get(field_name.as_str())
                        .cloned()
                        .cloned()
                        .or_else(|| {
                            missing_field = true;
                            diagnostics.push(MetaDiagnostic::new(
                                MetaDiagnosticCode::MissingFieldReference,
                                format!(
                                    "table `{}` index `{}` references unknown field `{}`",
                                    table.name, index.name, field_name
                                ),
                            ));
                            None
                        })
                })
                .collect();
            if missing_field {
                None
            } else {
                Some(LinkedIndexMeta {
                    meta: index,
                    fields,
                })
            }
        })
        .collect()
}

fn link_foreign_keys<'meta>(
    table: &'meta super::TableMeta,
    table_index: &HashMap<&'meta str, &'meta super::TableMeta>,
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
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
            // Keep linked FKs structurally trustworthy so validation and codegen do not repeat shape checks.
            if !target_index.unique && !target_index.primary {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` references target index `{}.{}` which is not unique",
                        table.name, foreign_key.name, target_table.name, target_index.name
                    ),
                ));
                continue;
            }
            if foreign_key.is_field_level() && target_index.fields.len() != 1 {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` references composite index `{}.{}`; field-level foreign key only supports single-field index",
                        table.name, foreign_key.name, target_table.name, target_index.name
                    ),
                ));
                continue;
            }
            let contains_list_source = fields
                .iter()
                .any(|field| matches!(field.linked_field, LinkedField::List(_)));
            if contains_list_source && (fields.len() != 1 || target_index.fields.len() != 1) {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` uses list source field; list foreign keys only support a single source field referencing a single-field target index",
                        table.name, foreign_key.name
                    ),
                ));
                continue;
            }
            if fields.len() != target_index.fields.len() {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` has {} source fields but target index `{}.{}` has {} fields",
                        table.name,
                        foreign_key.name,
                        fields.len(),
                        target_table.name,
                        target_index.name,
                        target_index.fields.len()
                    ),
                ));
                continue;
            }
            if !linked_foreign_key_types_compatible(&fields, target_table, target_index, enum_index)
            {
                diagnostics.push(MetaDiagnostic::new(
                    MetaDiagnosticCode::InvalidForeignKeyDeclaration,
                    format!(
                        "table `{}` foreign key `{}` has incompatible source and target key field types",
                        table.name, foreign_key.name
                    ),
                ));
                continue;
            }
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
                linked_field: foreign_key.linked_field,
            });
        }
    }
    linked_foreign_keys
}

fn linked_foreign_key_types_compatible(
    source_fields: &[LinkedFieldMeta<'_>],
    target_table: &super::TableMeta,
    target_index: &super::IndexMeta,
    enum_index: &HashMap<&str, &super::EnumMeta>,
) -> bool {
    source_fields
        .iter()
        .zip(target_index.fields.iter())
        .all(|(source_field, target_field_name)| {
            let Some(target_field) = target_table.field(target_field_name) else {
                return false;
            };
            let source_type = source_field.foreign_key_primitive_type();
            let target_type = target_field.primitive_type();
            match (source_type, target_type) {
                (Some(source), Some(target)) => source == target,
                (Some(_), None) | (None, Some(_)) => false,
                // Enums intentionally match by declared type, not by their underlying value representation.
                (None, None) => source_field
                    .foreign_key_enum_name()
                    .is_some_and(|source_name| {
                        source_name == target_field.field_type
                            && enum_index.contains_key(source_name)
                            && enum_index.contains_key(target_field.field_type.as_str())
                    }),
            }
        })
}

fn link_field_type<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    field: &'meta FieldMeta,
    struct_stack: &mut Vec<&'meta str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Option<LinkedFieldMeta<'meta>> {
    let linked_field = resolve_linked_field_type(
        enum_index,
        struct_index,
        field.field_type.as_str(),
        &field.name,
        struct_stack,
        diagnostics,
    )?;

    Some(LinkedFieldMeta {
        meta: field,
        linked_field,
    })
}

fn resolve_linked_field_type<'meta>(
    enum_index: &HashMap<&'meta str, &'meta super::EnumMeta>,
    struct_index: &HashMap<&'meta str, &'meta super::StructMeta>,
    field_type: &str,
    field_name: &str,
    struct_stack: &mut Vec<&'meta str>,
    diagnostics: &mut Vec<MetaDiagnostic>,
) -> Option<LinkedField<'meta>> {
    if let Some(element_type) = list_element_type_name(field_type) {
        let element = resolve_linked_field_type(
            enum_index,
            struct_index,
            element_type,
            field_name,
            struct_stack,
            diagnostics,
        )?;
        return Some(LinkedField::List(Box::new(element)));
    }

    let linked_field = if let Some(primitive) = PrimitiveFieldType::parse(field_type) {
        LinkedField::Primitive(primitive)
    } else if let Some(enum_meta) = enum_index.get(field_type) {
        LinkedField::Enum(enum_meta)
    } else if let Some(struct_meta) = struct_index.get(field_type) {
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
                field_name, field_type
            ),
        ));
        return None;
    };
    Some(linked_field)
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
