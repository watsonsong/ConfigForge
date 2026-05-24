use std::{fs, path::Path};

use crate::{ConfigForgeError, Result};

use super::{MetaDiagnostic, MetaModel, link_meta_model, validate_meta_model};

impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self> {
        let model: Self = toml::from_str(input).map_err(ConfigForgeError::ParseMetaString)?;
        finish_loading(model)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        load_meta_path(path.as_ref())
    }

    pub fn from_paths<I, P>(paths: I) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut loaded = Vec::new();
        for path in paths {
            loaded.push(read_meta_file(path.as_ref())?);
        }
        finish_loading(merge_models(loaded))
    }
}

pub fn load_meta_path(path: &Path) -> Result<MetaModel> {
    finish_loading(read_meta_file(path)?)
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
    };

    for mut model in models {
        merged.enums.append(&mut model.enums);
        merged.structs.append(&mut model.structs);
        merged.tables.append(&mut model.tables);
    }

    merged
}

fn finish_loading(model: MetaModel) -> Result<MetaModel> {
    let (_, mut diagnostics): (Option<super::LinkedMetaModel<'_>>, Vec<MetaDiagnostic>) =
        link_meta_model(&model);
    diagnostics.extend(validate_meta_model(&model));
    diagnostics.sort();
    diagnostics.dedup();

    if diagnostics.is_empty() {
        Ok(model)
    } else {
        Err(ConfigForgeError::MetaValidation(diagnostics))
    }
}
