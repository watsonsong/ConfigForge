// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    ArtifactProjectInfo,
    meta::{LinkedMetaModel, TargetTags},
};

use super::build::{apply_incoming_relations, relationship_graph, source_graph, table_doc};

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
    pub relationship_graph: SchemaRelationshipGraph,
    pub source_graph: SchemaSourceGraph,
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
    pub incoming_relations: Vec<SchemaRelationEdgeDoc>,
    pub fragment_graph: Option<SchemaFragmentGraphDoc>,
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
    pub target_table: String,
    pub target_index: String,
    pub target: String,
    pub cardinality: String,
    pub allow_empty: bool,
    pub linked_helper: Option<String>,
    pub description: Option<String>,
    pub visibility: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRelationshipGraph {
    pub nodes: Vec<SchemaTableNodeDoc>,
    pub edges: Vec<SchemaRelationEdgeDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaTableNodeDoc {
    pub table: String,
    pub kind: String,
    pub description: Option<String>,
    pub source_summary: Option<String>,
    pub primary_index: Option<String>,
    pub field_count: usize,
    pub index_count: usize,
    pub relation_count: usize,
    pub visibility: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaRelationEdgeDoc {
    pub source_table: String,
    pub source_field: String,
    pub target_table: String,
    pub target_index: String,
    pub target_display: String,
    pub cardinality: String,
    pub allow_empty: bool,
    pub linked_helper: Option<String>,
    pub visibility: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSourceGraph {
    pub nodes: Vec<SchemaSourceNodeDoc>,
    pub edges: Vec<SchemaSourceTableEdgeDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSourceNodeDoc {
    pub path: String,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaSourceTableEdgeDoc {
    pub source_path: String,
    pub provider: String,
    pub table: String,
    pub role: String,
    pub fragments: Vec<String>,
    pub layout: String,
    pub visibility: String,
    pub header_row: usize,
    pub data_start_row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFragmentGraphDoc {
    pub table: String,
    pub base_fields: Vec<String>,
    pub fragments: Vec<SchemaFragmentDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFragmentDoc {
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub payload: String,
    pub source_hints: Vec<String>,
    pub fields: Vec<SchemaFragmentFieldDoc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaFragmentFieldDoc {
    pub path: String,
    pub type_display: String,
    pub visibility: String,
    pub description: Option<String>,
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
        project: &ArtifactProjectInfo,
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
        let mut tables = linked_meta
            .tables
            .iter()
            .filter_map(|table| table_doc(linked_meta, table, &options))
            .collect::<Vec<_>>();
        let relationship_graph = relationship_graph(&tables);
        apply_incoming_relations(&mut tables, &relationship_graph.edges);
        let source_graph = source_graph(linked_meta, &tables, &options);
        Self {
            metadata,
            tables,
            relationship_graph,
            source_graph,
        }
    }
}
