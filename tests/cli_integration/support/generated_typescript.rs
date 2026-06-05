// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{assert_command_success, command_exists};

pub(crate) fn verify_generated_typescript(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.ts"),
        r#"
            import { GameData, RuntimeDataLoader, RuntimeDataReader, itemsNameText } from "./generated";

            class MockReader implements RuntimeDataReader {
                private readonly fields: string[] = [];

                constructor(private readonly path: string) {}

                enterField(name: string): boolean {
                    if (name === "stack" || name === "attributes" || name === "skill") {
                        return false;
                    }
                    this.fields.push(name);
                    return true;
                }

                enterIndex(index: number): void {
                    if (this.path === "items" && this.currentField() === "rows" && index === 0) {
                        this.fields.push("");
                        return;
                    }
                    if (this.path === "game_rules" && this.currentField() === "starter_item_ids" && index < 2) {
                        this.fields.push("");
                        return;
                    }
                    throw new Error(`unexpected index: ${index}`);
                }

                exit(): void {
                    if (this.fields.length === 0) {
                        throw new Error("unexpected exit");
                    }
                    this.fields.pop();
                }

                length(): number {
                    if (this.path === "items" && this.currentField() === "rows") {
                        return 1;
                    }
                    if ((this.path === "classes" || this.path === "class_levels") && this.currentField() === "rows") {
                        return 0;
                    }
                    if (this.path === "game_rules" && this.currentField() === "starter_item_ids") {
                        return 2;
                    }
                    throw new Error("unexpected length");
                }

                asNumber(): number {
                    if (this.path === "items" && this.currentField() === "id") { return 1; }
                    if (this.path === "items" && this.currentField() === "price") { return 10; }
                    if (this.path === "game_rules" && this.currentField() === "max_player_level") { return 60; }
                    if (this.path === "game_rules" && this.currentField() === "starter_gold") { return 500; }
                    if (this.path === "game_rules" && this.currentField() === "default_class_id") { return 1; }
                    if (this.path === "game_rules" && this.currentField() === "") { return 2001; }
                    if (this.path === "game_rules" && this.currentField() === "stamina_cap") { return 120; }
                    if (this.path === "game_rules" && this.currentField() === "daily_reset_hour") { return 5; }
                    throw new Error("unexpected number");
                }

                asBool(): boolean {
                    if (this.path === "game_rules" && this.currentField() === "pvp_enabled") { return true; }
                    throw new Error("unexpected bool");
                }

                asString(): string {
                    if (this.path === "items" && this.currentField() === "name") { return "Potion"; }
                    throw new Error("unexpected string");
                }

                private currentField(): string {
                    return this.fields[this.fields.length - 1] ?? "";
                }
            }

            const loader: RuntimeDataLoader = {
                open(path: string): RuntimeDataReader {
                    if (path === "items" || path === "classes" || path === "class_levels" || path === "game_rules") {
                        return new MockReader(path);
                    }
                    throw new Error(`unexpected path: ${path}`);
                },
            };

            async function main(): Promise<void> {
                const data = await GameData.load(loader);
                const item = data.items.getById(1);
                if (!item || item.name !== "Potion") {
                    throw new Error("generated TypeScript query failed");
                }
                const translated = itemsNameText(item, (context, msgid) => context === "items.name" && msgid === "Potion" ? "Potion CN" : undefined);
                if (translated !== "Potion CN") {
                    throw new Error("generated TypeScript text translation failed");
                }
                if (data.gameRules.maxPlayerLevel !== 60 || !data.gameRules.pvpEnabled) {
                    throw new Error("generated TypeScript object table failed");
                }
                if (data.gameRules.defaultClassId !== 1 || data.gameRules.starterItemIds.length !== 2) {
                    throw new Error("generated TypeScript list field failed");
                }
            }

            main().catch((error) => {
                console.error(error);
                throw error;
            });
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
    } else {
        eprintln!("skipping generated TypeScript type check: tsc/npx not found");
    }
}
pub(crate) fn verify_generated_typescript_global_i18n(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("i18n.ts"),
        r#"
            export function globalTranslate(context: string | undefined, msgid: string): string {
              return context === "items.name" && msgid === "Potion" ? "Potion CN" : msgid;
            }
        "#,
    )
    .expect("generated TypeScript i18n module should be written");
    fs::write(
        generated_dir.join("global_i18n_check.ts"),
        r#"
            import { itemsNameText } from "./generated";

            const translated = itemsNameText({ name: "Potion" });
            if (translated !== "Potion CN") {
              throw new Error("generated TypeScript global i18n failed");
            }
        "#,
    )
    .expect("generated TypeScript global i18n check should be written");
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
                "include": ["generated.ts", "i18n.ts", "global_i18n_check.ts"]
            }
        "#,
    )
    .expect("generated TypeScript global i18n tsconfig should be written");

    if command_exists("tsc") {
        assert_command_success(
            StdCommand::new("tsc")
                .args(["--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript global i18n type check should run"),
            "generated TypeScript global i18n tsc",
        );
    } else if command_exists("npx") {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript global i18n npx type check should run"),
            "generated TypeScript global i18n npx tsc",
        );
    } else {
        eprintln!("skipping generated TypeScript global i18n type check: tsc/npx not found");
    }
}
