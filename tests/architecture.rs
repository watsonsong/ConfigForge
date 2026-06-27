// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, path::Path};

#[test]
fn architecture_module_boundaries_match_outline() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

    let rules = [
        ModuleRule {
            path: &["diagnostic"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "meta",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["meta"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["data"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "asset",
                "build",
                "code_artifact",
                "data_artifact",
                "generator",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["runtime_contract"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "project",
                "validation",
            ],
        },
        ModuleRule {
            path: &["asset"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["validation"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "build",
                "code_artifact",
                "data_artifact",
                "generator",
                "project",
                "runtime_contract",
            ],
        },
        ModuleRule {
            path: &["artifact"],
            forbidden_modules: &[
                "artifact_info",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "meta",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["artifact_info.rs"],
            forbidden_modules: &[
                "artifact",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "meta",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["data_artifact"],
            forbidden_modules: &["asset", "build", "generator", "project", "validation"],
        },
        ModuleRule {
            path: &["code_artifact"],
            forbidden_modules: &[
                "asset",
                "build",
                "data",
                "data_artifact",
                "generator",
                "project",
                "validation",
            ],
        },
        ModuleRule {
            path: &["generator", "schema_doc"],
            forbidden_modules: &[
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["project"],
            forbidden_modules: &[
                "artifact",
                "asset",
                "build",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "runtime_contract",
                "validation",
            ],
        },
        ModuleRule {
            path: &["build"],
            forbidden_modules: &["code_artifact", "data_artifact"],
        },
        ModuleRule {
            path: &["cli.rs"],
            forbidden_modules: &[
                "artifact",
                "artifact_info",
                "asset",
                "code_artifact",
                "data",
                "data_artifact",
                "generator",
                "meta",
                "project",
                "runtime_contract",
                "validation",
            ],
        },
    ];

    for rule in rules {
        assert_module_does_not_depend_on(&rule.root(&src), rule.forbidden_modules);
    }
}

struct ModuleRule<'a> {
    path: &'a [&'a str],
    forbidden_modules: &'a [&'a str],
}

impl ModuleRule<'_> {
    fn root(&self, src: &Path) -> std::path::PathBuf {
        self.path
            .iter()
            .fold(src.to_path_buf(), |path, component| path.join(component))
    }
}

fn assert_module_does_not_depend_on(module_root: &Path, forbidden_modules: &[&str]) {
    let mut violations = Vec::new();
    for file in rust_files(module_root) {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", file.display()));
        let production_source = production_source(&content);
        for module in forbidden_modules {
            if contains_crate_module_reference(production_source, module) {
                violations.push(format!("{} -> crate::{module}", file.display()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "module boundary violations:\n{}\nkeep rules in Documentation/Implementation/01-module-outline.md synchronized with this architecture guard",
        violations.join("\n")
    );
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
