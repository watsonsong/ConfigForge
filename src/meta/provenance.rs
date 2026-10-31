// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use crate::DiagnosticLocation;

use super::MetaDiagnostic;

pub type DeclarationSources = BTreeMap<String, Vec<DiagnosticLocation>>;

#[derive(Deserialize)]
struct Declaration {
    name: toml::Spanned<String>,
    #[serde(default)]
    fields: Vec<Declaration>,
}

pub(crate) fn declaration_sources(content: &str, path: Option<&Path>) -> DeclarationSources {
    #[derive(Deserialize)]
    struct Document {
        #[serde(default)]
        tables: Vec<Declaration>,
        #[serde(default)]
        structs: Vec<Declaration>,
        #[serde(default)]
        enums: Vec<Declaration>,
        #[serde(default)]
        asset_kinds: Vec<Declaration>,
    }
    let Ok(document) = toml::from_str::<Document>(content) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    let line_starts = std::iter::once(0)
        .chain(content.match_indices('\n').map(|(offset, _)| offset + 1))
        .collect::<Vec<_>>();
    for (category, items) in [
        ("tables", document.tables),
        ("structs", document.structs),
        ("enums", document.enums),
        ("asset_kinds", document.asset_kinds),
    ] {
        for item in items {
            let key = format!("{category}/{}", item.name.get_ref());
            result
                .entry(key.clone())
                .or_insert_with(Vec::new)
                .push(location(content, &line_starts, path, &item.name));
            for field in item.fields {
                result
                    .entry(format!("{key}/fields/{}", field.name.get_ref()))
                    .or_insert_with(Vec::new)
                    .push(location(content, &line_starts, path, &field.name));
            }
        }
    }
    result
}

fn location(
    content: &str,
    line_starts: &[usize],
    path: Option<&Path>,
    name: &toml::Spanned<String>,
) -> DiagnosticLocation {
    let offset = name.span().start;
    let line = line_starts.partition_point(|start| *start <= offset) - 1;
    DiagnosticLocation {
        source_path: path.map(Path::to_path_buf),
        source_row_number: Some(line + 1),
        source_column_number: Some(content[line_starts[line]..offset].chars().count() + 1),
        ..Default::default()
    }
}

pub(crate) fn enrich_diagnostics(
    sources: &DeclarationSources,
    key: &str,
    diagnostics: &mut [MetaDiagnostic],
) {
    let Some(locations) = sources.get(key) else {
        return;
    };
    for diagnostic in diagnostics {
        if diagnostic.location.source_row_number.is_some() {
            continue;
        }
        if let Some(primary) = locations.last() {
            diagnostic.location.source_path = primary.source_path.clone();
            diagnostic.location.source_row_number = primary.source_row_number;
            diagnostic.location.source_column_number = primary.source_column_number;
        }
        if locations.len() > 1 {
            diagnostic
                .related_locations
                .extend_from_slice(&locations[..locations.len() - 1]);
        }
    }
}
