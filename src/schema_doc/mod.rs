// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::PathBuf, str::FromStr};

use crate::{
    ConfigForgeError, Result,
    artifact::ArtifactSink,
    codegen::CodeExportProject,
    meta::{
        AssetKindMeta, ExportCondition, LinkedField, LinkedFieldMeta, LinkedForeignKeyMeta,
        LinkedMetaModel, LinkedStructType, LinkedTableMeta, PrimitiveFieldType, TableKind,
        TargetTags, ValidatorMeta,
    },
    project::lexical_normalize_path,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaDocFormat {
    Markdown,
}

impl SchemaDocFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
        }
    }
}

impl fmt::Display for SchemaDocFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Markdown => "markdown",
        })
    }
}

impl FromStr for SchemaDocFormat {
    type Err = ConfigForgeError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "markdown" | "md" => Ok(Self::Markdown),
            other => Err(ConfigForgeError::Validation(format!(
                "unsupported schema_doc format `{other}`; supported formats: markdown"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocExportConfig {
    pub format: SchemaDocFormat,
    pub output: PathBuf,
    pub index_file: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocOptions {
    pub title: Option<String>,
    pub project_name: Option<String>,
    pub project_path: Option<PathBuf>,
    pub target: Option<String>,
    pub target_tags: TargetTags,
    pub generated_at: String,
    pub include_hidden: bool,
    pub include_internal: bool,
    pub include_struct_details: bool,
    pub include_asset_rules: bool,
    pub include_i18n: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDoc {
    pub metadata: SchemaDocMetadata,
    pub tables: Vec<SchemaTableDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocMetadata {
    pub title: String,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub target: Option<String>,
    pub target_tags: Vec<String>,
    pub generated_at: String,
    pub config_forge_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaTableDoc {
    pub name: String,
    pub kind: String,
    pub description: Option<String>,
    pub source_summary: Option<String>,
    pub visibility: String,
    pub row_export_summary: String,
    pub primary_index: Option<String>,
    pub fields: Vec<SchemaFieldDoc>,
    pub indexes: Vec<SchemaIndexDoc>,
    pub relations: Vec<SchemaRelationDoc>,
    pub asset_fields: Vec<SchemaAssetFieldDoc>,
    pub text_fields: Vec<SchemaTextFieldDoc>,
    pub validators: Vec<SchemaValidatorDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFieldDoc {
    pub path: String,
    pub type_display: String,
    pub required: bool,
    pub default_display: Option<String>,
    pub visibility: String,
    pub description: Option<String>,
    pub asset_kind: Option<String>,
    pub enters_pot: bool,
    pub validators: Vec<SchemaValidatorDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIndexDoc {
    pub name: String,
    pub fields: Vec<String>,
    pub unique: bool,
    pub primary: bool,
    pub visibility: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRelationDoc {
    pub field: String,
    pub target: String,
    pub cardinality: String,
    pub linked_helper: Option<String>,
    pub description: Option<String>,
    pub visibility: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaAssetFieldDoc {
    pub field: String,
    pub asset_kind: Option<String>,
    pub required: bool,
    pub visibility: String,
    pub rules: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaTextFieldDoc {
    pub field: String,
    pub visibility: String,
    pub enters_pot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaValidatorDoc {
    pub scope: String,
    pub name: String,
    pub summary: String,
    pub severity: String,
    pub description: Option<String>,
}

impl SchemaDoc {
    pub fn from_meta(
        linked_meta: &LinkedMetaModel<'_>,
        project: &CodeExportProject,
        options: SchemaDocOptions,
    ) -> Self {
        let title = options.title.clone().unwrap_or_else(|| {
            project
                .name
                .clone()
                .map(|name| format!("{name} Schema"))
                .unwrap_or_else(|| "ConfigForge Schema".to_owned())
        });
        let metadata = SchemaDocMetadata {
            title,
            project_name: options
                .project_name
                .clone()
                .or_else(|| project.name.clone()),
            project_path: options
                .project_path
                .as_ref()
                .map(|path| path.display().to_string()),
            target: options.target.clone(),
            target_tags: options
                .target_tags
                .iter()
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            generated_at: options.generated_at.clone(),
            config_forge_version: env!("CARGO_PKG_VERSION").to_owned(),
        };
        let tables = linked_meta
            .tables
            .iter()
            .filter_map(|table| table_doc(linked_meta, table, &options))
            .collect();
        Self { metadata, tables }
    }
}

pub fn write_schema_doc_to_sink(
    doc: &SchemaDoc,
    config: &SchemaDocExportConfig,
    sink: &ArtifactSink,
) -> Result<()> {
    match config.format {
        SchemaDocFormat::Markdown => {
            let paths = schema_doc_output_paths(doc, config)?;
            sink.write_text(&paths.index_path, render_schema_doc_index_markdown(doc))?;
            for (table, path) in doc.tables.iter().zip(paths.table_paths) {
                sink.write_text(path, render_schema_table_markdown(doc, table))?;
            }
            Ok(())
        }
    }
}

pub fn schema_doc_output_paths(
    doc: &SchemaDoc,
    config: &SchemaDocExportConfig,
) -> Result<SchemaDocOutputPaths> {
    validate_file_name(&config.index_file, "schema_doc index_file")?;
    let index_path = lexical_normalize_path(&config.output.join(&config.index_file));
    let mut seen = std::collections::BTreeSet::new();
    if !seen.insert(index_path.clone()) {
        return Err(ConfigForgeError::Validation(format!(
            "schema_doc output path `{}` is duplicated",
            index_path.display()
        )));
    }
    let mut table_paths = Vec::with_capacity(doc.tables.len());
    for table in &doc.tables {
        let file_name = table_file_name(table, config.format);
        validate_file_name(&file_name, "schema_doc table file")?;
        let path = lexical_normalize_path(&config.output.join(file_name));
        if !seen.insert(path.clone()) {
            return Err(ConfigForgeError::Validation(format!(
                "schema_doc output path `{}` is duplicated",
                path.display()
            )));
        }
        table_paths.push(path);
    }
    Ok(SchemaDocOutputPaths {
        index_path,
        table_paths,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDocOutputPaths {
    pub index_path: PathBuf,
    pub table_paths: Vec<PathBuf>,
}

pub fn render_schema_doc_index_markdown(doc: &SchemaDoc) -> String {
    let mut out = String::new();
    push_line(
        &mut out,
        &format!("# {}", escape_markdown_text(&doc.metadata.title)),
    );
    push_line(&mut out, "");
    push_line(&mut out, "| Property | Value |");
    push_line(&mut out, "| --- | --- |");
    push_kv(&mut out, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut out, "Target", doc.metadata.target.as_deref());
    push_kv(
        &mut out,
        "Tags",
        Some(&display_list_or_dash(&doc.metadata.target_tags)),
    );
    push_kv(&mut out, "Generated at", Some(&doc.metadata.generated_at));
    push_kv(
        &mut out,
        "ConfigForge version",
        Some(&doc.metadata.config_forge_version),
    );
    push_line(&mut out, "");
    push_line(&mut out, "## Tables");
    push_line(&mut out, "");
    push_line(
        &mut out,
        "| Table | Kind | Description | Fields | Indexes | Relations | Visibility |",
    );
    push_line(&mut out, "| --- | --- | --- | ---: | ---: | ---: | --- |");
    for table in &doc.tables {
        let file_name = table_file_name(table, SchemaDocFormat::Markdown);
        push_line(
            &mut out,
            &format!(
                "| [{}]({}) | {} | {} | {} | {} | {} | {} |",
                escape_markdown_table_cell(&table.name),
                escape_markdown_link_target(&file_name),
                escape_markdown_table_cell(&table.kind),
                display_cell(table.description.as_deref()),
                table.fields.len(),
                table.indexes.len(),
                table.relations.len(),
                escape_markdown_table_cell(&table.visibility),
            ),
        );
    }
    out
}

pub fn render_schema_table_markdown(doc: &SchemaDoc, table: &SchemaTableDoc) -> String {
    let mut out = String::new();
    push_line(
        &mut out,
        &format!("# {}", escape_markdown_text(&table.name)),
    );
    push_line(&mut out, "");
    if let Some(description) = &table.description {
        push_line(&mut out, &escape_markdown_text(description));
        push_line(&mut out, "");
    }
    push_line(&mut out, "| Property | Value |");
    push_line(&mut out, "| --- | --- |");
    push_kv(&mut out, "Project", doc.metadata.project_name.as_deref());
    push_kv(&mut out, "Kind", Some(&table.kind));
    push_kv(&mut out, "Source", table.source_summary.as_deref());
    push_kv(&mut out, "Visibility", Some(&table.visibility));
    push_kv(&mut out, "Row export", Some(&table.row_export_summary));
    push_kv(&mut out, "Primary index", table.primary_index.as_deref());
    push_line(&mut out, "");

    push_line(&mut out, "## Fields");
    push_line(&mut out, "");
    push_line(
        &mut out,
        "| Field | Type | Required | Default | Export | Description |",
    );
    push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
    for field in &table.fields {
        push_line(
            &mut out,
            &format!(
                "| {} | {} | {} | {} | {} | {} |",
                escape_markdown_table_cell(&field.path),
                escape_markdown_table_cell(&field.type_display),
                yes_no(field.required),
                display_cell(field.default_display.as_deref()),
                escape_markdown_table_cell(&field.visibility),
                display_cell(field.description.as_deref()),
            ),
        );
    }
    push_line(&mut out, "");

    push_line(&mut out, "## Indexes");
    push_line(&mut out, "");
    if table.indexes.is_empty() {
        push_line(&mut out, "No indexes.");
    } else {
        push_line(
            &mut out,
            "| Name | Fields | Unique | Primary | Export | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
        for index in &table.indexes {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&index.name),
                    escape_markdown_table_cell(&index.fields.join(", ")),
                    yes_no(index.unique),
                    yes_no(index.primary),
                    escape_markdown_table_cell(&index.visibility),
                    display_cell(index.description.as_deref()),
                ),
            );
        }
    }
    push_line(&mut out, "");

    push_line(&mut out, "## Relations");
    push_line(&mut out, "");
    if table.relations.is_empty() {
        push_line(&mut out, "No relations.");
    } else {
        push_line(
            &mut out,
            "| Field | Target | Cardinality | Linked Helper | Export | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- | --- |");
        for relation in &table.relations {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&relation.field),
                    escape_markdown_table_cell(&relation.target),
                    escape_markdown_table_cell(&relation.cardinality),
                    display_cell(relation.linked_helper.as_deref()),
                    escape_markdown_table_cell(&relation.visibility),
                    display_cell(relation.description.as_deref()),
                ),
            );
        }
    }
    push_line(&mut out, "");

    if !table.asset_fields.is_empty() {
        push_line(&mut out, "## Assets");
        push_line(&mut out, "");
        push_line(
            &mut out,
            "| Field | Asset Kind | Required | Export | Rules |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for asset in &table.asset_fields {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&asset.field),
                    display_cell(asset.asset_kind.as_deref()),
                    yes_no(asset.required),
                    escape_markdown_table_cell(&asset.visibility),
                    display_cell(asset.rules.as_deref()),
                ),
            );
        }
        push_line(&mut out, "");
    }

    if !table.text_fields.is_empty() {
        push_line(&mut out, "## Text / POT");
        push_line(&mut out, "");
        push_line(&mut out, "| Field | Export | POT |");
        push_line(&mut out, "| --- | --- | --- |");
        for text in &table.text_fields {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} |",
                    escape_markdown_table_cell(&text.field),
                    escape_markdown_table_cell(&text.visibility),
                    yes_no(text.enters_pot),
                ),
            );
        }
        push_line(&mut out, "");
    }

    if !table.validators.is_empty()
        || table
            .fields
            .iter()
            .any(|field| !field.validators.is_empty())
    {
        push_line(&mut out, "## Validators");
        push_line(&mut out, "");
        push_line(
            &mut out,
            "| Scope | Name | Summary | Severity | Description |",
        );
        push_line(&mut out, "| --- | --- | --- | --- | --- |");
        for validator in table.validators.iter().chain(
            table
                .fields
                .iter()
                .flat_map(|field| field.validators.iter()),
        ) {
            push_line(
                &mut out,
                &format!(
                    "| {} | {} | {} | {} | {} |",
                    escape_markdown_table_cell(&validator.scope),
                    escape_markdown_table_cell(&validator.name),
                    escape_markdown_table_cell(&validator.summary),
                    escape_markdown_table_cell(&validator.severity),
                    display_cell(validator.description.as_deref()),
                ),
            );
        }
        push_line(&mut out, "");
    }

    out
}

fn table_doc(
    linked_meta: &LinkedMetaModel<'_>,
    table: &LinkedTableMeta<'_>,
    options: &SchemaDocOptions,
) -> Option<SchemaTableDoc> {
    let table_visible = table.meta.is_exported_for(&options.target_tags);
    if !table_visible && !options.include_hidden {
        return None;
    }
    let fields = field_docs(linked_meta, table, options);
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
        asset_fields,
        text_fields,
        validators,
    })
}

fn field_docs(
    linked_meta: &LinkedMetaModel<'_>,
    table: &LinkedTableMeta<'_>,
    options: &SchemaDocOptions,
) -> Vec<SchemaFieldDoc> {
    let mut fields = Vec::new();
    for field in &table.fields {
        collect_field_doc(linked_meta, &mut fields, field, None, options);
    }
    if options.include_struct_details {
        for fragment in &table.fragments {
            if !fragment.meta.is_exported_for(&options.target_tags) && !options.include_hidden {
                continue;
            }
            for field in &fragment.fields {
                collect_field_doc(
                    linked_meta,
                    &mut fields,
                    field,
                    Some(&fragment.meta.name),
                    options,
                );
            }
        }
    }
    fields
}

fn collect_field_doc(
    linked_meta: &LinkedMetaModel<'_>,
    fields: &mut Vec<SchemaFieldDoc>,
    field: &LinkedFieldMeta<'_>,
    prefix: Option<&str>,
    options: &SchemaDocOptions,
) {
    let field_visible = field.meta.is_exported_for(&options.target_tags);
    if !field_visible && !options.include_hidden {
        return;
    }
    if !field.meta.export && !options.include_internal {
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
            collect_struct_children(linked_meta, fields, struct_type, &path, options);
        }
    }
}

fn collect_struct_children(
    linked_meta: &LinkedMetaModel<'_>,
    fields: &mut Vec<SchemaFieldDoc>,
    struct_type: &LinkedStructType<'_>,
    parent: &str,
    options: &SchemaDocOptions,
) {
    let _ = linked_meta;
    for child in &struct_type.fields {
        collect_field_doc(linked_meta, fields, child, Some(parent), options);
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
            let index_visible = index.meta.is_exported_for(&options.target_tags)
                && index
                    .fields
                    .iter()
                    .all(|field| visible_fields.contains(field.meta.name.as_str()));
            if !index_visible && !options.include_hidden {
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
            let target_table_visible = linked_meta
                .table(foreign_key.target_table_name)
                .is_some_and(|target_table| {
                    target_table.meta.is_exported_for(&options.target_tags)
                });
            let relation_visible = foreign_key.is_exported_for(&options.target_tags)
                && target_table_visible
                && foreign_key
                    .fields
                    .iter()
                    .all(|field| visible_fields.contains(field.meta.name.as_str()));
            if !relation_visible && !options.include_hidden {
                return None;
            }
            Some(SchemaRelationDoc {
                field: foreign_key
                    .fields
                    .iter()
                    .map(|field| field.meta.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                target: relation_target_display(linked_meta, foreign_key),
                cardinality: relation_cardinality(foreign_key).to_owned(),
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
    display
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

fn table_file_name(table: &SchemaTableDoc, format: SchemaDocFormat) -> String {
    format!("{}.{}", table.name, format.extension())
}

fn validate_file_name(value: &str, label: &str) -> Result<()> {
    let path = std::path::Path::new(value);
    let valid = !value.is_empty()
        && path.file_name().and_then(|name| name.to_str()) == Some(value)
        && path.components().count() == 1
        && !matches!(
            path.components().next(),
            Some(std::path::Component::ParentDir | std::path::Component::CurDir)
        );
    if valid {
        Ok(())
    } else {
        Err(ConfigForgeError::Validation(format!(
            "{label} `{value}` must be a plain file name"
        )))
    }
}

pub fn schema_doc_export_config_from_options(
    output: PathBuf,
    mut options: BTreeMap<String, toml::Value>,
) -> Result<SchemaDocExportConfig> {
    let format = take_schema_doc_string(&mut options, "format")?
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(SchemaDocFormat::Markdown);
    let index_file = take_schema_doc_string(&mut options, "index_file")?
        .unwrap_or_else(|| format!("index.{}", format.extension()));
    validate_file_name(&index_file, "schema_doc index_file")?;
    let title = take_schema_doc_string(&mut options, "title")?;
    let unsupported = options.keys().cloned().collect::<Vec<_>>();
    if let Some(key) = unsupported.first() {
        return Err(ConfigForgeError::Validation(format!(
            "schema_doc export task has unknown option `{key}`"
        )));
    }
    Ok(SchemaDocExportConfig {
        format,
        output,
        index_file,
        title,
    })
}

fn take_schema_doc_string(
    options: &mut BTreeMap<String, toml::Value>,
    key: &str,
) -> Result<Option<String>> {
    options
        .remove(key)
        .map(|value| {
            value.try_into().map_err(|source| {
                ConfigForgeError::Validation(format!("invalid schema_doc option `{key}`: {source}"))
            })
        })
        .transpose()
}

fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}

fn push_kv(out: &mut String, key: &str, value: Option<&str>) {
    push_line(
        out,
        &format!(
            "| {} | {} |",
            escape_markdown_table_cell(key),
            display_cell(value)
        ),
    );
}

fn display_cell(value: Option<&str>) -> String {
    value
        .filter(|value| !value.is_empty())
        .map(escape_markdown_table_cell)
        .unwrap_or_else(|| "-".to_owned())
}

fn display_list_or_dash(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn escape_markdown_text(value: &str) -> String {
    value.replace('\r', "").replace('\n', "\n")
}

fn escape_markdown_table_cell(value: &str) -> String {
    escape_markdown_text(value).replace('|', "\\|")
}

fn escape_markdown_link_target(value: &str) -> String {
    value.replace(' ', "%20").replace(')', "%29")
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use crate::{
        artifact::{ArtifactWriteMode, ArtifactWriter},
        codegen::CodeExportProject,
        meta::{MetaModel, TargetTags},
    };

    use super::*;

    fn build_doc(input: &str, tags: &[&str]) -> SchemaDoc {
        let meta = MetaModel::from_toml_str(input).expect("meta should parse");
        let linked = meta.link().expect("meta should link");
        SchemaDoc::from_meta(
            &linked,
            &CodeExportProject {
                name: Some("demo".to_owned()),
                desc: None,
                copyright: None,
            },
            SchemaDocOptions {
                title: None,
                project_name: None,
                project_path: None,
                target: Some("client".to_owned()),
                target_tags: TargetTags::new(tags.iter().copied()),
                generated_at: "2026-06-07T00:00:00Z".to_owned(),
                include_hidden: false,
                include_internal: false,
                include_struct_details: true,
                include_asset_rules: true,
                include_i18n: true,
            },
        )
    }

    #[test]
    fn builds_schema_doc_with_fields_indexes_relations_and_enrichment() {
        let doc = build_doc(
            r#"
                [[asset_kinds]]
                name = "ui_icon"
                extensions = [".png", ".webp"]
                name_prefixes = ["UI_"]

                [[tables]]
                name = "classes"
                source = "classes.csv"
                desc = "职业表"

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables]]
                name = "items"
                source = "items.csv"
                desc = "道具|表"
                row_export = { required_tags_field = "tags" }

                [[tables.fields]]
                name = "id"
                type = "int"
                required = true
                desc = "道具 ID"

                [[tables.fields]]
                name = "name"
                type = "text"
                required = true
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "icon"
                type = "asset"
                asset_kind = "ui_icon"
                export_when = { any = ["client"] }

                [[tables.fields]]
                name = "tags"
                type = "string"

                [[tables.fields]]
                name = "class_id"
                type = "int"
                foreign_key = { target_table = "classes", target_index = "primary", linked_field = "class" }

                [[tables.indexes]]
                name = "primary"
                fields = ["id"]
                unique = true
                primary = true

                [[tables.fields.validators]]
                kind = "range"
                min = 1
                max = 999
            "#,
            &["client"],
        );

        assert_eq!(doc.tables.len(), 2);
        let items = doc
            .tables
            .iter()
            .find(|table| table.name == "items")
            .unwrap();
        assert_eq!(items.fields.len(), 5);
        assert_eq!(items.indexes[0].name, "primary");
        assert_eq!(items.relations[0].target, "classes.id");
        assert_eq!(
            items.asset_fields[0].rules.as_deref(),
            Some("extensions .png/.webp, prefix UI_")
        );
        assert_eq!(items.text_fields[0].field, "name");
        assert!(
            items
                .fields
                .iter()
                .any(|field| !field.validators.is_empty())
        );

        let markdown = render_schema_table_markdown(&doc, items);
        let index_markdown = render_schema_doc_index_markdown(&doc);
        assert!(index_markdown.contains("道具\\|表"));
        assert!(markdown.contains("## Assets"));
        assert!(markdown.contains("## Text / POT"));
        assert!(markdown.contains("range(max=999, min=1)"));
    }

    #[test]
    fn filters_by_target_and_keeps_docs_only_artifact_paths_deterministic() {
        let doc = build_doc(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"

                [[tables.fields]]
                name = "server_note"
                type = "string"
                export_when = { any = ["server"] }

                [[tables.fields]]
                name = "internal"
                type = "string"
                export = false
            "#,
            &["client"],
        );
        let items = &doc.tables[0];
        assert_eq!(items.fields.len(), 1);
        assert_eq!(items.fields[0].path, "id");

        let config = SchemaDocExportConfig {
            format: SchemaDocFormat::Markdown,
            output: PathBuf::from("out/docs"),
            index_file: "index.md".to_owned(),
            title: None,
        };
        let paths = schema_doc_output_paths(&doc, &config).expect("paths should plan");
        assert_eq!(paths.index_path, PathBuf::from("out/docs/index.md"));
        assert_eq!(paths.table_paths, vec![PathBuf::from("out/docs/items.md")]);
    }

    #[test]
    fn rejects_invalid_schema_doc_options_and_duplicate_paths() {
        let error = schema_doc_export_config_from_options(
            PathBuf::from("out"),
            BTreeMap::from([("format".to_owned(), toml::Value::String("html".to_owned()))]),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unsupported schema_doc format"));

        let doc = SchemaDoc {
            metadata: SchemaDocMetadata {
                title: "Schema".to_owned(),
                project_name: None,
                project_path: None,
                target: None,
                target_tags: Vec::new(),
                generated_at: "now".to_owned(),
                config_forge_version: "test".to_owned(),
            },
            tables: vec![SchemaTableDoc {
                name: "index".to_owned(),
                kind: "CollectionTable".to_owned(),
                description: None,
                source_summary: None,
                visibility: "all".to_owned(),
                row_export_summary: "no".to_owned(),
                primary_index: None,
                fields: Vec::new(),
                indexes: Vec::new(),
                relations: Vec::new(),
                asset_fields: Vec::new(),
                text_fields: Vec::new(),
                validators: Vec::new(),
            }],
        };
        let config = SchemaDocExportConfig {
            format: SchemaDocFormat::Markdown,
            output: PathBuf::from("out"),
            index_file: "index.md".to_owned(),
            title: None,
        };
        let error = schema_doc_output_paths(&doc, &config).unwrap_err();
        assert!(error.to_string().contains("duplicated"));
    }

    #[test]
    fn writes_markdown_artifacts_to_sink() {
        let doc = build_doc(
            r#"
                [[tables]]
                name = "items"
                source = "items.csv"

                [[tables.fields]]
                name = "id"
                type = "int"
            "#,
            &["client"],
        );
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config = SchemaDocExportConfig {
            format: SchemaDocFormat::Markdown,
            output: temp_dir.path().join("docs"),
            index_file: "index.md".to_owned(),
            title: None,
        };
        let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
        write_schema_doc_to_sink(&doc, &config, &sink).expect("write docs");
        sink.finish().expect("finish sink");
        assert!(temp_dir.path().join("docs/index.md").exists());
        assert!(temp_dir.path().join("docs/items.md").exists());
    }
}
