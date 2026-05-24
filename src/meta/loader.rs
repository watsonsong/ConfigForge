use std::{fs, path::Path};

use crate::{ConfigForgeError, Result};

use super::MetaModel;

impl MetaModel {
    pub fn from_toml_str(input: &str) -> Result<Self> {
        let model: Self = toml::from_str(input).map_err(ConfigForgeError::ParseMetaString)?;
        finish_loading_model(model)
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
        finish_loading_model(merge_models(loaded))
    }

    pub fn finish_loading(self) -> Result<Self> {
        finish_loading_model(self)
    }
}

pub fn load_meta_path(path: &Path) -> Result<MetaModel> {
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
    };

    for mut model in models {
        merged.enums.append(&mut model.enums);
        merged.structs.append(&mut model.structs);
        merged.tables.append(&mut model.tables);
    }

    merged
}

fn finish_loading_model(model: MetaModel) -> Result<MetaModel> {
    if let Err(diagnostics) = model.link() {
        return Err(ConfigForgeError::MetaValidation(diagnostics));
    }
    Ok(model)
}
