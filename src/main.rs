// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use config_forge::cli;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
