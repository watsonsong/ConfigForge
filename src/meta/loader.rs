use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{ConfigForgeError, Result};

use super::{link_meta_model, validate_meta_model, MetaDiagnostic, MetaModel};

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
        let mut visited = BTreeSet::new();
        let mut loaded = Vec::new();
        for path in paths {
            load_recursive(path.as_ref(), &mut visited, &mut loaded)?;
        }
        finish_loading(merge_models(loaded))
    }
}

pub fn load_meta_path(path: &Path) -> Result<MetaModel> {
    let mut visited = BTreeSet::new();
    let mut loaded = Vec::new();
    load_recursive(path, &mut visited, &mut loaded)?;
    finish_loading(merge_models(loaded))
}

fn load_recursive(
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    loaded: &mut Vec<MetaModel>,
) -> Result<()> {
    let canonical_path = fs::canonicalize(path).map_err(|source| ConfigForgeError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    if !visited.insert(canonical_path.clone()) {
        return Ok(());
    }

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

    let base_dir = canonical_path.parent().unwrap_or_else(|| Path::new("."));
    for import in &model.imports {
        load_recursive(&base_dir.join(import), visited, loaded)?;
    }
    loaded.push(model);
    Ok(())
}

fn merge_models(models: Vec<MetaModel>) -> MetaModel {
    let mut merged = MetaModel {
        imports: Vec::new(),
        project: None,
        enums: Vec::new(),
        structs: Vec::new(),
        tables: Vec::new(),
    };

    for mut model in models {
        if merged.project.is_none() {
            merged.project = model.project.take();
        }
        merged.imports.append(&mut model.imports);
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
