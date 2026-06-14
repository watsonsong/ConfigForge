// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::{fs, process::Command as StdCommand};

use super::{
    assert_command_success, command_exists, network_allowed, require_any_tool_or_skip,
    skip_optional_test,
};

pub(crate) fn verify_generated_typescript(generated_dir: &std::path::Path) {
    fs::write(
        generated_dir.join("compile_check.ts"),
        r#"
            import { GameData, RuntimeDataLoader, RuntimeDataReader, itemsNameText } from "./generated";

            class MockReader implements RuntimeDataReader {
                private readonly fields: string[] = [];

                constructor(private readonly path: string) {}

                enterRequiredField(name: string): void {
                    if (!this.enterField(name)) {
                        throw new Error(`missing required field: ${name}`);
                    }
                }

                enterOptionalField(name: string): boolean {
                    return this.enterField(name);
                }

                exitField(): void {
                    this.exit();
                }

                private enterField(name: string): boolean {
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
                    if (this.path === "game_rules" && this.currentField() === "item_ids" && index < 2) {
                        this.fields.push(index === 0 ? "starter_reward.item_ids[0]" : "starter_reward.item_ids[1]");
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

                readRowCount(): number {
                    return this.length();
                }

                readListLength(): number {
                    return this.length();
                }

                readEnumIndex(variants: readonly string[]): number {
                    const value = this.asString();
                    const index = variants.indexOf(value);
                    if (index < 0) {
                        throw new Error(`unknown enum value: ${value}`);
                    }
                    return index;
                }

                arrayElementsArePositional(): boolean {
                    return false;
                }

                beginRecord(_optionalSlotCount: number): void {}

                endRecord(): void {}

                private length(): number {
                    if (this.path === "items" && this.currentField() === "rows") {
                        return 1;
                    }
                    if ((this.path === "classes" || this.path === "class_levels" || this.path === "quests" || this.path === "loot_pools") && this.currentField() === "rows") {
                        return 0;
                    }
                    if (this.path === "game_rules" && (this.currentField() === "starter_item_ids" || this.currentField() === "item_ids")) {
                        return 2;
                    }
                    throw new Error("unexpected length");
                }

                asInt64(): number {
                    return this.numberValue();
                }

                asFloat64(): number {
                    return this.numberValue();
                }

                private numberValue(): number {
                    if (this.path === "items" && this.currentField() === "id") { return 1; }
                    if (this.path === "items" && this.currentField() === "price") { return 10; }
                    if (this.path === "game_rules" && this.currentField() === "max_player_level") { return 60; }
                    if (this.path === "game_rules" && this.currentField() === "starter_gold") { return 500; }
                    if (this.path === "game_rules" && this.currentField() === "default_class_id") { return 1; }
                    if (this.path === "game_rules" && this.currentField() === "") { return 2001; }
                    if (this.path === "game_rules" && this.currentField() === "xp") { return 0; }
                    if (this.path === "game_rules" && this.currentField() === "gold") { return 500; }
                    if (this.path === "game_rules" && this.currentField() === "starter_reward.item_ids[0]") { return 2001; }
                    if (this.path === "game_rules" && this.currentField() === "starter_reward.item_ids[1]") { return 2002; }
                    if (this.path === "game_rules" && this.parentField() === "ambient_tint" && this.currentField() === "r") { return 51; }
                    if (this.path === "game_rules" && this.parentField() === "ambient_tint" && this.currentField() === "g") { return 102; }
                    if (this.path === "game_rules" && this.parentField() === "ambient_tint" && this.currentField() === "b") { return 153; }
                    if (this.path === "game_rules" && this.parentField() === "ambient_tint" && this.currentField() === "a") { return 255; }
                    if (this.path === "game_rules" && this.parentField() === "spawn_position" && this.currentField() === "x") { return 1.5; }
                    if (this.path === "game_rules" && this.parentField() === "spawn_position" && this.currentField() === "y") { return 2.5; }
                    if (this.path === "game_rules" && this.parentField() === "spawn_position" && this.currentField() === "z") { return 3.5; }
                    if (this.path === "game_rules" && this.parentField() === "grid_size" && this.currentField() === "x") { return 16; }
                    if (this.path === "game_rules" && this.parentField() === "grid_size" && this.currentField() === "y") { return 9; }
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

                private parentField(): string {
                    return this.fields[this.fields.length - 2] ?? "";
                }
            }

            const loader: RuntimeDataLoader = {
                open(path: string): RuntimeDataReader {
                    if (path === "items" || path === "classes" || path === "class_levels" || path === "quests" || path === "loot_pools" || path === "game_rules") {
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
                if (data.gameRules.starterReward.gold !== 500 || data.gameRules.starterReward.itemIds.length !== 2) {
                    throw new Error("generated TypeScript fixed reward field failed");
                }
                if (data.gameRules.ambientTint.r !== 51 || data.gameRules.ambientTint.g !== 102 || data.gameRules.ambientTint.b !== 153 || data.gameRules.ambientTint.a !== 255) {
                    throw new Error("generated TypeScript color field failed");
                }
                if (data.gameRules.spawnPosition.x !== 1.5 || data.gameRules.spawnPosition.y !== 2.5 || data.gameRules.spawnPosition.z !== 3.5) {
                    throw new Error("generated TypeScript vector field failed");
                }
                if (data.gameRules.gridSize.x !== 16 || data.gameRules.gridSize.y !== 9) {
                    throw new Error("generated TypeScript int vector field failed");
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
    } else if command_exists("npx") && network_allowed() {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript npx type check should run"),
            "generated TypeScript npx tsc",
        );
    } else if command_exists("npx") {
        skip_optional_test(
            "generated TypeScript type check",
            "npx fallback requires CONFIG_FORGE_ALLOW_NETWORK=1",
        );
    } else {
        let _ = require_any_tool_or_skip("generated TypeScript type check", &["tsc", "npx"]);
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
    } else if command_exists("npx") && network_allowed() {
        assert_command_success(
            StdCommand::new("npx")
                .args(["--yes", "-p", "typescript", "tsc", "--project"])
                .arg(generated_dir.join("tsconfig.json"))
                .output()
                .expect("generated TypeScript global i18n npx type check should run"),
            "generated TypeScript global i18n npx tsc",
        );
    } else if command_exists("npx") {
        skip_optional_test(
            "generated TypeScript global i18n type check",
            "npx fallback requires CONFIG_FORGE_ALLOW_NETWORK=1",
        );
    } else {
        let _ = require_any_tool_or_skip(
            "generated TypeScript global i18n type check",
            &["tsc", "npx"],
        );
    }
}
