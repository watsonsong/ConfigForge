// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fs, path::Path};

use super::{absolute_fixture_path, standard_core_meta_imports};

pub(crate) fn write_stability_project(
    temp_root: &Path,
    name: &str,
    output_root: &Path,
) -> std::path::PathBuf {
    let project_path = temp_root.join(format!("ConfigForge_{name}.toml"));
    fs::write(
        &project_path,
        format!(
            r#"
                imports = [{}]

                [project]
                input = "{}"

                [[targets]]
                name = "client"
                tags = ["client"]

                [[targets.exports]]
                output = "{}"
                type = "json"

                [[targets.exports]]
                type = "rust"
                output = "{}"
                file_header = "// Stable generated fixture\n"
            "#,
            standard_core_meta_imports(),
            absolute_fixture_path("data"),
            output_root.join("data").display(),
            output_root.join("code/rust").display()
        ),
    )
    .expect("project config should be written");
    project_path
}
pub(crate) fn collect_file_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    collect_file_tree_into(root, root, &mut files);
    files
}
pub(crate) fn collect_file_tree_into(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) {
    if !current.exists() {
        return;
    }
    let mut entries = fs::read_dir(current)
        .expect("directory should be readable")
        .map(|entry| entry.expect("directory entry should be readable"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_file_tree_into(root, &path, files);
        } else {
            let relative_path = path
                .strip_prefix(root)
                .expect("file should be under root")
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            files.insert(
                relative_path,
                fs::read(path).expect("file should be readable"),
            );
        }
    }
}
pub(crate) fn file_tree_entries(files: &BTreeMap<String, Vec<u8>>) -> Vec<(String, usize)> {
    files
        .iter()
        .map(|(path, content)| (path.clone(), content.len()))
        .collect()
}
pub(crate) fn dry_run_artifact_entries(stdout: &str, output_root: &Path) -> Vec<(String, usize)> {
    let mut entries = Vec::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let Some((path, metadata)) = rest.rsplit_once(" (") else {
            continue;
        };
        let Some((bytes, written)) = metadata.strip_suffix(')').and_then(|metadata| {
            let (bytes, written) = metadata.split_once(" bytes, written: ")?;
            Some((bytes.parse::<usize>().ok()?, written))
        }) else {
            continue;
        };
        assert_eq!(written, "false");
        let relative_path = Path::new(path)
            .strip_prefix(output_root)
            .expect("artifact path should be under output root")
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        entries.push((relative_path, bytes));
    }
    entries.sort();
    entries
}
