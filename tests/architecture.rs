// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

#[test]
fn architecture_module_boundaries_match_outline() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    assert_module_does_not_depend_on(
        &src.join("diagnostic"),
        &["meta", "data", "validation", "generator", "build"],
    );
    assert_module_does_not_depend_on(
        &src.join("validation"),
        &["code_artifact", "data_artifact", "generator", "build"],
    );
    assert_module_does_not_depend_on(
        &src.join("code_artifact"),
        &["data", "data_artifact", "validation", "generator", "build"],
    );
    assert_module_does_not_depend_on(
        &src.join("data_artifact"),
        &["validation", "generator", "build"],
    );
    assert_module_does_not_depend_on(
        &src.join("generator").join("schema_doc"),
        &[
            "data",
            "validation",
            "code_artifact",
            "data_artifact",
            "build",
            "project",
        ],
    );
}

fn assert_module_does_not_depend_on(module_root: &Path, forbidden_modules: &[&str]) {
    for file in rust_files(module_root) {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", file.display()));
        let production_source = production_source(&content);
        for module in forbidden_modules {
            assert!(
                !contains_crate_module_reference(production_source, module),
                "`{}` must not depend on crate module `{module}`; keep rules in Documentation/Implementation/01-module-outline.md synchronized with this architecture guard",
                file.display()
            );
        }
    }
}

fn rust_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files);
    files.sort();
    files
}

fn collect_rust_files(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    if path.is_dir() {
        for entry in fs::read_dir(path)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()))
        {
            let entry = entry.expect("source directory entry should be readable");
            collect_rust_files(&entry.path(), files);
        }
        return;
    }
    if path.extension().and_then(|extension| extension.to_str()) == Some("rs")
        && path.file_name().and_then(|name| name.to_str()) != Some("tests.rs")
    {
        files.push(path.to_path_buf());
    }
}

fn production_source(content: &str) -> &str {
    content.split("#[cfg(test)]").next().unwrap_or(content)
}

fn contains_crate_module_reference(source: &str, module: &str) -> bool {
    source.contains(&format!("crate::{module}"))
        || source
            .match_indices("crate::{")
            .any(|(start, _)| grouped_import_contains(&source[start..], module))
}

fn grouped_import_contains(source: &str, module: &str) -> bool {
    let Some(end) = source.find("};") else {
        return false;
    };
    source[..end]
        .split(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .any(|token| token == module)
}
