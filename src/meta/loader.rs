// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

use crate::{ConfigForgeError, Result};

use super::MetaModel;

impl MetaModel {
    /// Parses a TOML Meta document and validates it before returning the owned raw model.
    ///
    /// The returned [`MetaModel`] is still the serde-owned raw schema, but loading has
    /// already run `link()` to catch link-time and structural diagnostics. Callers that
    /// already have a raw model from another source should use [`MetaModel::finish_loading`]
    /// to apply the same contract, or [`MetaModel::link`] when they need a borrowed linked view.
    pub fn from_toml_str(input: &str) -> Result<Self> {
        let model: Self = toml::from_str(input).map_err(ConfigForgeError::ParseMetaString)?;
        finish_loading_model(model)
    }

    /// Loads a Meta file from disk and validates it before returning the owned raw model.
    ///
    /// See [`MetaModel::from_toml_str`] for the raw-vs-linked lifecycle contract.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        load_meta_path(path.as_ref())
    }

    /// Loads, merges, and validates multiple Meta files before returning the owned raw model.
    ///
    /// Files are concatenated at the raw schema level; validation then runs on the merged
    /// model so duplicate names and cross-file references are checked as one schema.
    pub fn from_paths<I, P>(paths: I) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut loaded = Vec::new();
        for path in paths {
            loaded.push(read_meta_file(path.as_ref())?);
        }
        finish_loading_model(merge_models(loaded))
    }

    /// Validates an already constructed raw Meta model and returns it unchanged on success.
    ///
    /// This is useful for callers that assemble `MetaModel` programmatically but still want
    /// the same load-time validation performed by `from_toml_str` / `from_path`.
    pub fn finish_loading(self) -> Result<Self> {
        finish_loading_model(self)
    }
}

fn load_meta_path(path: &Path) -> Result<MetaModel> {
    finish_loading_model(read_meta_file(path)?)
}

fn read_meta_file(path: &Path) -> Result<MetaModel> {
    let canonical_path = fs::canonicalize(path).map_err(|source| ConfigForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;

    let content =
        fs::read_to_string(&canonical_path).map_err(|source| ConfigForgeError::ReadFile {
            path: canonical_path.clone(),
            source,
        })?;
    let model: MetaModel =
        toml::from_str(&content).map_err(|source| ConfigForgeError::ParseMeta {
            path: canonical_path.clone(),
            source,
        })?;
    Ok(model)
}

fn merge_models(models: Vec<MetaModel>) -> MetaModel {
    let mut merged = MetaModel {
        enums: Vec::new(),
        structs: Vec::new(),
        tables: Vec::new(),
        asset_kinds: Vec::new(),
    };

    for mut model in models {
        merged.enums.append(&mut model.enums);
        merged.structs.append(&mut model.structs);
        merged.tables.append(&mut model.tables);
        merged.asset_kinds.append(&mut model.asset_kinds);
    }

    merged
}

fn finish_loading_model(model: MetaModel) -> Result<MetaModel> {
    if let Err(diagnostics) = model.link() {
        return Err(ConfigForgeError::MetaValidation(diagnostics));
    }
    Ok(model)
}
