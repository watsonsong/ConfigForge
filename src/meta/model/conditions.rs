// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ExportCondition {
    #[serde(default)]
    pub any: Vec<String>,
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub none: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct RowExportPolicy {
    #[serde(default)]
    pub required_tags_field: Option<String>,
    #[serde(default)]
    pub excluded_tags_field: Option<String>,
    #[serde(default)]
    #[serde(rename = "match")]
    pub match_mode: TagMatchMode,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TagMatchMode {
    #[default]
    Any,
    All,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetTags {
    tags: BTreeSet<String>,
}

impl ExportCondition {
    pub fn matches(&self, target: &TargetTags) -> bool {
        (self.any.is_empty() || self.any.iter().any(|tag| target.contains(tag)))
            && self.all.iter().all(|tag| target.contains(tag))
            && self.none.iter().all(|tag| !target.contains(tag))
    }
}

impl RowExportPolicy {
    pub fn matches<'a>(
        &self,
        target: &TargetTags,
        value_of: impl Fn(&str) -> Option<&'a str>,
    ) -> bool {
        if let Some(field) = &self.excluded_tags_field
            && parse_tag_list(value_of(field))
                .iter()
                .any(|tag| target.contains(tag))
        {
            return false;
        }

        let required_tags = self
            .required_tags_field
            .as_ref()
            .map(|field| parse_tag_list(value_of(field)))
            .unwrap_or_default();
        if required_tags.is_empty() {
            return true;
        }

        match self.match_mode {
            TagMatchMode::Any => required_tags.iter().any(|tag| target.contains(tag)),
            TagMatchMode::All => required_tags.iter().all(|tag| target.contains(tag)),
        }
    }
}

impl TargetTags {
    pub fn new(tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            tags: tags
                .into_iter()
                .filter_map(|tag| normalize_tag(tag.into()))
                .collect(),
        }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn contains(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.tags.iter().map(String::as_str)
    }
}

fn parse_tag_list(value: Option<&str>) -> Vec<&str> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect()
}

pub(crate) fn normalize_tag(tag: String) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_owned())
    }
}
