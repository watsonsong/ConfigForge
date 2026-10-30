// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, fs, path::Path};

use syn::visit::{self, Visit};

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
        let dependencies = production_dependencies(&content);
        for module in forbidden_modules {
            if dependencies.contains(*module) {
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

fn production_dependencies(content: &str) -> BTreeSet<String> {
    let file = syn::parse_file(content).expect("source should be valid Rust syntax");
    let mut visitor = DependencyVisitor::default();
    visitor.visit_file(&file);
    visitor.dependencies
}

#[derive(Default)]
struct DependencyVisitor {
    dependencies: BTreeSet<String>,
}

fn test_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .parse_args::<syn::Path>()
                .is_ok_and(|path| path.is_ident("test"))
    })
}

impl<'ast> Visit<'ast> for DependencyVisitor {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        let attrs = match item {
            syn::Item::Const(item) => &item.attrs,
            syn::Item::Enum(item) => &item.attrs,
            syn::Item::ExternCrate(item) => &item.attrs,
            syn::Item::Fn(item) => &item.attrs,
            syn::Item::ForeignMod(item) => &item.attrs,
            syn::Item::Impl(item) => &item.attrs,
            syn::Item::Macro(item) => &item.attrs,
            syn::Item::Mod(item) => &item.attrs,
            syn::Item::Static(item) => &item.attrs,
            syn::Item::Struct(item) => &item.attrs,
            syn::Item::Trait(item) => &item.attrs,
            syn::Item::TraitAlias(item) => &item.attrs,
            syn::Item::Type(item) => &item.attrs,
            syn::Item::Union(item) => &item.attrs,
            syn::Item::Use(item) => &item.attrs,
            _ => return,
        };
        if !test_only(attrs) {
            visit::visit_item(self, item);
        }
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        let attrs = match item {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => return,
        };
        if !test_only(attrs) {
            visit::visit_impl_item(self, item);
        }
    }

    fn visit_trait_item(&mut self, item: &'ast syn::TraitItem) {
        let attrs = match item {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => return,
        };
        if !test_only(attrs) {
            visit::visit_trait_item(self, item);
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut segments = path.segments.iter();
        if segments
            .next()
            .is_some_and(|segment| segment.ident == "crate")
        {
            if let Some(module) = segments.next() {
                self.dependencies.insert(module.ident.to_string());
            }
        }
        visit::visit_path(self, path);
    }

    fn visit_item_use(&mut self, item: &'ast syn::ItemUse) {
        self.record_use(&item.tree, false);
    }
}

impl DependencyVisitor {
    fn record_use(&mut self, tree: &syn::UseTree, in_crate: bool) {
        match tree {
            syn::UseTree::Group(group) => {
                for item in &group.items {
                    self.record_use(item, in_crate);
                }
            }
            syn::UseTree::Path(path) if !in_crate && path.ident == "crate" => {
                self.record_use(&path.tree, true);
            }
            syn::UseTree::Path(path) if in_crate => {
                self.dependencies.insert(path.ident.to_string());
            }
            syn::UseTree::Name(name) if in_crate => {
                self.dependencies.insert(name.ident.to_string());
            }
            syn::UseTree::Rename(rename) if in_crate => {
                self.dependencies.insert(rename.ident.to_string());
            }
            _ => {}
        }
    }
}

#[test]
fn production_after_test_module_is_checked() {
    let dependencies =
        production_dependencies("#[cfg(test)] mod tests; use crate::build::BuildOptions;");
    assert!(dependencies.contains("build"));
}

#[test]
fn test_items_and_strings_do_not_create_dependencies() {
    let dependencies = production_dependencies(
        r#"
        #[cfg(test)] mod tests { use crate::build::BuildOptions; }
        #[cfg(test)] use crate::generator::GeneratorRegistry;
        const EXAMPLE: &str = "crate::validation";
        // crate::data
        struct Example;
        impl Example {
            #[cfg(test)] fn example() { crate::project::example(); }
        }
    "#,
    );
    assert!(dependencies.is_empty());
}

#[test]
fn grouped_imports_and_paths_only_record_root_modules() {
    let dependencies = production_dependencies(
        r#"
        use crate::{meta::{data, MetaModel}, data as source};
        fn example() { crate::validation::example(); }
    "#,
    );
    assert_eq!(
        dependencies,
        BTreeSet::from(["data".into(), "meta".into(), "validation".into()])
    );
}
