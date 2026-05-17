use std::{fs, process::Command as StdCommand};

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn builds_code_only_target_without_loading_data() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let out_dir = temp_dir.path().join("generated_code");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = ["{}"]

                [project]
                name = "standard_project"

                [[targets]]
                name = "code_only"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "code_only",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `code_only` built"));

    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

#[test]
fn builds_target_from_project_config() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let cpp_out_dir = temp_dir.path().join("out/code/cpp");
    let ts_out_dir = temp_dir.path().join("out/code/typescript");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = ["{}"]

                [project]
                name = "standard_project"
                desc = "Standard fixture"
                copyright = "Copyright Fixture"

                [[targets]]
                name = "client"

                [targets.data]
                input = "{}"
                output = "{}"
                format = "json"

                [[targets.code]]
                language = "rust"
                output = "{}"

                [[targets.code]]
                language = "cpp"
                output = "{}"

                [[targets.code]]
                language = "typescript"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display(),
            cpp_out_dir.display(),
            ts_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client` built"));

    assert!(data_out_dir.join("items.json").exists());
    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    let cpp_code = fs::read_to_string(cpp_out_dir.join("generated.hpp")).expect("cpp code");
    let ts_code = fs::read_to_string(ts_out_dir.join("generated.ts")).expect("ts code");
    assert!(rust_code.contains("pub struct ItemsRow"));
    assert!(rust_code.contains("pub trait RuntimeDataLoader"));
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.json\""));
    assert!(!rust_code.contains("RuntimeDataFormat"));
    assert!(rust_code.contains("pub fn get_by_id"));
    assert!(cpp_code.contains("class IRuntimeDataLoader"));
    assert!(cpp_code.contains("static constexpr const char* DataFile = \"items.json\""));
    assert!(!cpp_code.contains("RuntimeDataFormat"));
    assert!(cpp_code.contains("const ItemsRow* GetById"));
    assert!(ts_code.contains("export interface RuntimeDataLoader"));
    assert!(ts_code.contains("static readonly dataFile = \"items.json\""));
    assert!(!ts_code.contains("RuntimeDataFormat"));
    assert!(ts_code.contains("getById(id: number): ItemsRow | undefined"));

    verify_generated_rust_crate(&rust_out_dir, false);
    verify_generated_cpp_header(&cpp_out_dir);
    verify_generated_typescript(&ts_out_dir);
}

#[test]
fn builds_messagepack_target_and_codegen_uses_messagepack_files() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let data_out_dir = temp_dir.path().join("out/data");
    let rust_out_dir = temp_dir.path().join("out/code/rust");
    let project_path = temp_dir.path().join("ConfigForge.toml");
    fs::write(
        &project_path,
        format!(
            r#"
                imports = ["{}"]

                [project]
                name = "standard_project"

                [[targets]]
                name = "client_messagepack"

                [targets.data]
                input = "{}"
                output = "{}"
                format = "messagepack"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            absolute_fixture_path("meta.toml"),
            absolute_fixture_path("data"),
            data_out_dir.display(),
            rust_out_dir.display()
        ),
    )
    .expect("project config should be written");

    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_messagepack",
    ]);

    cmd.assert().success().stdout(predicate::str::contains(
        "target `client_messagepack` built",
    ));

    let messagepack_path = data_out_dir.join("items.msgpack");
    assert!(messagepack_path.exists());
    let messagepack_bytes = fs::read(messagepack_path).expect("messagepack data");
    assert!(!messagepack_bytes.is_empty());
    let payload: serde_json::Value =
        rmp_serde::from_slice(&messagepack_bytes).expect("messagepack should decode");
    assert_eq!(payload["meta"]["name"], "items");
    assert!(payload["meta"].get("fields").is_none());
    assert!(!payload["rows"]
        .as_array()
        .expect("rows should be array")
        .is_empty());

    let rust_code = fs::read_to_string(rust_out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub const DATA_FILE: &'static str = \"items.msgpack\""));
    assert!(rust_code.contains("pub trait RuntimeDataLoader"));
    assert!(!rust_code.contains("RuntimeDataFormat"));

    verify_generated_rust_crate(&rust_out_dir, false);
}

#[test]
fn builds_target_from_imported_project_config_fragments() {
    let temp_dir = tempfile::tempdir().expect("temp dir should be created");
    let config_dir = temp_dir.path().join("Config");
    let meta_dir = config_dir.join("Meta");
    let targets_dir = config_dir.join("Targets");
    fs::create_dir_all(&meta_dir).expect("meta dir should be created");
    fs::create_dir_all(&targets_dir).expect("config dirs should be created");

    let out_dir = temp_dir.path().join("generated/rust");
    fs::write(
        config_dir.join("ConfigForge.toml"),
        r#"
            imports = ["Meta/items.toml", "Targets/client.toml"]

            [project]
            name = "imported_project"
        "#,
    )
    .expect("root project config should be written");
    fs::write(
        meta_dir.join("items.toml"),
        fs::read_to_string(absolute_fixture_path("meta.toml")).expect("fixture meta should exist"),
    )
    .expect("meta config fragment should be written");
    fs::write(
        targets_dir.join("client.toml"),
        format!(
            r#"
                [[targets]]
                name = "client_imported"

                [[targets.code]]
                language = "rust"
                output = "{}"
            "#,
            out_dir.display()
        ),
    )
    .expect("target config fragment should be written");

    let imported_project_path = config_dir.join("ConfigForge.toml");
    let mut cmd = Command::cargo_bin("config_forge").expect("binary should build");
    cmd.args([
        "--project",
        imported_project_path.to_str().expect("path should be utf8"),
        "--target",
        "client_imported",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("target `client_imported` built"));

    let rust_code = fs::read_to_string(out_dir.join("generated.rs")).expect("rust code");
    assert!(rust_code.contains("pub struct ItemsRow"));
}

fn absolute_fixture_path(relative_path: &str) -> String {
    std::env::current_dir()
        .expect("current dir")
        .join("tests/fixtures/standard_project")
        .join(relative_path)
        .display()
        .to_string()
}

fn verify_generated_rust_crate(generated_dir: &std::path::Path, derive_serde: bool) {
    fs::write(
        generated_dir.join("Cargo.toml"),
        format!(
            r#"
                [package]
                name = "generated_config_check"
                version = "0.1.0"
                edition = "2021"

                [lib]
                path = "lib.rs"

                {dependencies}
            "#,
            dependencies = if derive_serde {
                "[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\nserde_json = \"1.0\""
            } else {
                ""
            }
        ),
    )
    .expect("generated rust Cargo.toml should be written");
    fs::write(
        generated_dir.join("lib.rs"),
        format!(
            r#"
                include!("generated.rs");

                struct MockLoader;

                impl RuntimeDataLoader for MockLoader {{
                    fn load_items_rows(&self, path: &str) -> Result<Vec<ItemsRow>, String> {{
                        if path != ItemsTable::DATA_FILE {{
                            return Err(format!("unexpected path: {{path}}"));
                        }}
                        Ok(vec![ItemsRow {{
                            id: 1,
                            name: "Potion".to_owned(),
                            price: 10,
                            enabled: true,
                        }}])
                    }}
                }}

                #[test]
                fn generated_code_loads_and_queries_rows() {{
                    let data = GameData::load(&MockLoader).expect("generated data should load");
                    let item = data.items.get_by_id(1).expect("item should be indexed");
                    assert_eq!(item.name, "Potion");
                    assert_eq!(data.items.rows().len(), 1);
                }}

                {serde_assertion}
            "#,
            serde_assertion = if derive_serde {
                "#[test]\nfn generated_rows_support_serde() {\n    let _ = serde_json::to_string(&ItemsRow { id: 1, name: \"Potion\".to_owned(), price: 10, enabled: true }).unwrap();\n}"
            } else {
                ""
            }
        ),
    )
    .expect("generated rust lib.rs should be written");

    let output = StdCommand::new("cargo")
        .args(["test", "--quiet", "--manifest-path"])
        .arg(generated_dir.join("Cargo.toml"))
        .output()
        .expect("generated Rust cargo test should run");
    assert_command_success(output, "generated Rust cargo test");
}

fn verify_generated_cpp_header(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.cpp"),
        r#"
            #include "generated.hpp"

            int main() {
                ItemsRow row{};
                row.id = 1;
                row.name = "Potion";
                row.price = 10;
                row.enabled = true;
                (void)row;
                return 0;
            }
        "#,
    )
    .expect("generated C++ compile check should be written");

    if command_exists("c++") {
        let output = StdCommand::new("c++")
            .args([
                "-std=c++17",
                "-fsyntax-only",
                generated_dir
                    .join("compile_check.cpp")
                    .to_str()
                    .expect("utf8 path"),
            ])
            .output()
            .expect("generated C++ compile check should run");
        assert_command_success(output, "generated C++ header compile");
    }
}

fn verify_generated_typescript(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.ts"),
        r#"
            import { GameData, ItemsRow, RuntimeDataLoader } from "./generated";

            const loader: RuntimeDataLoader = {
                loadItemsRows(path: string): ItemsRow[] {
                    if (path !== "items.json") {
                        throw new Error(`unexpected path: ${path}`);
                    }
                    return [{ id: 1, name: "Potion", price: 10, enabled: true }];
                },
            };

            async function main(): Promise<void> {
                const data = await GameData.load(loader);
                const item = data.items.getById(1);
                if (!item || item.name !== "Potion") {
                    throw new Error("generated TypeScript query failed");
                }
            }

            void main();
        "#,
    )
    .expect("generated TypeScript compile check should be written");
    fs::write(
        generated_dir.join("tsconfig.json"),
        r#"
            {
                "compilerOptions": {
                    "strict": true,
                    "target": "ES2020",
                    "module": "ESNext",
                    "moduleResolution": "Bundler",
                    "noEmit": true,
                    "skipLibCheck": true
                },
                "include": ["generated.ts", "compile_check.ts"]
            }
        "#,
    )
    .expect("generated TypeScript tsconfig should be written");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript type check should run"),
            "generated TypeScript tsc",
        );
    } else if command_exists("npx") {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript npx type check should run"),
            "generated TypeScript npx tsc",
        );
    }
}

fn command_exists(command: &str) -> bool {
    StdCommand::new(command)
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn assert_command_success(output: std::process::Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
