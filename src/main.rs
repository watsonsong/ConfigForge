// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

fn main() {
    if let Err(err) = config_forge::run_cli() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
