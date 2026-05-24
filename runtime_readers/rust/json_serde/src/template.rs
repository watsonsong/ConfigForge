use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use crate::runtime_reader_core::ValueReader;

pub const SUPPORTED_CONFIG_FORGE_READER_CONTRACT: &str = "^1.0";

#[derive(Debug, Clone)]
pub struct FsJsonRuntimeDataLoader {
    data_dir: PathBuf,
}

impl FsJsonRuntimeDataLoader {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    pub fn open_reader(&self, path: &str) -> Result<ValueReader, String> {
        let bytes = fs::read(safe_join(&self.data_dir, path)?).map_err(|error| error.to_string())?;
        let value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(ValueReader::new(value))
    }
}

fn safe_join(root: &Path, logical_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(logical_path);
    if path.is_absolute() {
        return Err(format!("runtime data path must be relative: {logical_path}"));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(format!(
                        "runtime data path escapes data root: {logical_path}"
                    ));
                }
            }
            _ => {
                return Err(format!("unsupported runtime data path: {logical_path}"));
            }
        }
    }

    Ok(root.join(normalized))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, process};

    #[test]
    fn fs_json_loader_opens_payload() {
        let dir = env::temp_dir().join(format!(
            "config_forge_json_loader_test_{}",
            process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp runtime data dir should be created");

        let payload = serde_json::json!({ "rows": [{ "id": "7" }] });
        fs::write(dir.join("items.json"), serde_json::to_vec(&payload).unwrap()).unwrap();

        let reader = FsJsonRuntimeDataLoader::new(&dir)
            .open_reader("items.json")
            .unwrap();
        let row = reader
            .field("rows")
            .unwrap()
            .expect("rows")
            .index(0)
            .unwrap();
        assert_eq!(row.field("id").unwrap().unwrap().as_i64().unwrap(), 7);

        fs::remove_dir_all(&dir).expect("temp runtime data dir should be removed");
    }

    #[test]
    fn fs_json_loader_rejects_path_traversal() {
        let loader = FsJsonRuntimeDataLoader::new("/tmp/config_forge_runtime_root");
        assert!(loader.open_reader("../secret.json").unwrap_err().contains("escapes"));
        assert!(loader.open_reader("/tmp/secret.json").unwrap_err().contains("relative"));
    }
}
