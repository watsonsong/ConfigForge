use std::{fs, path::PathBuf};

use crate::runtime_reader_core::ValueReader;

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
        let bytes = fs::read(self.data_dir.join(path)).map_err(|error| error.to_string())?;
        let value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(ValueReader::new(value))
    }
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
}
