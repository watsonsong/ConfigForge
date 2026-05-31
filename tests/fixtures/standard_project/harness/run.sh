#!/usr/bin/env bash
# Copyright (c) 2026 ConfigForge contributors
# SPDX-License-Identifier: Apache-2.0

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
FIXTURE_ROOT=$(cd "${SCRIPT_DIR}/.." && pwd)
REPO_ROOT=$(cd "${FIXTURE_ROOT}/../../.." && pwd)
TMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/configforge-standard-harness.XXXXXX")

cleanup() {
  if [[ "${CONFIG_FORGE_KEEP_HARNESS_TMP:-}" == "1" ]]; then
    echo "Keeping temporary directory: ${TMP_ROOT}"
  else
    rm -rf "${TMP_ROOT}"
  fi
}
trap cleanup EXIT

log() {
  printf '\n==> %s\n' "$1"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

select_tsc_command() {
  if command -v tsc >/dev/null 2>&1; then
    TSC_CMD=(tsc)
  elif command -v npx >/dev/null 2>&1; then
    TSC_CMD=(npx --yes -p typescript tsc)
  else
    echo "Missing TypeScript compiler: tsc or npx is required" >&2
    exit 1
  fi
}

run_rust_harness() {
  require_command cargo
  local work_dir="${TMP_ROOT}/rust"
  mkdir -p "${work_dir}"

  log "Rust harness: JSON / MessagePack / binary"
  sed \
    -e "s#../../../../../runtime/rust/reader_core#${REPO_ROOT}/runtime/rust/reader_core#g" \
    "${SCRIPT_DIR}/rust/Cargo.toml" > "${work_dir}/Cargo.toml"
  printf 'config-forge-runtime-binary-reader = { path = "%s" }\n' \
    "${REPO_ROOT}/runtime/rust/binary_reader" >> "${work_dir}/Cargo.toml"

  cp "${SCRIPT_DIR}/rust/lib.rs" "${work_dir}/lib.rs"
  cp "${REPO_ROOT}/runtime/rust/serde_json_reader/src/runtime_reader.rs" \
    "${work_dir}/json_runtime_reader.rs"
  cp "${REPO_ROOT}/runtime/rust/serde_msgpack_reader/src/runtime_reader.rs" \
    "${work_dir}/msgpack_runtime_reader.rs"
  cp "${FIXTURE_ROOT}/out/server_code/rust/reference_game_config.rs" \
    "${work_dir}/generated.rs"

  for edition in 2021 2024; do
    sed -i.bak "s/edition = \"2024\"/edition = \"${edition}\"/" "${work_dir}/Cargo.toml"
    rm -f "${work_dir}/Cargo.toml.bak"

    CARGO_TARGET_DIR="${work_dir}/target-${edition}-json" \
    JSON_DATA_DIR="${FIXTURE_ROOT}/out/client_data/json" \
    MSGPACK_DATA_DIR="${FIXTURE_ROOT}/out/server_data/msgpack" \
      cargo test --quiet --manifest-path "${work_dir}/Cargo.toml" \
        generated_rust_loads_exported_json_and_queries_indexes

    CARGO_TARGET_DIR="${work_dir}/target-${edition}-messagepack" \
    JSON_DATA_DIR="${FIXTURE_ROOT}/out/client_data/json" \
    MSGPACK_DATA_DIR="${FIXTURE_ROOT}/out/server_data/msgpack" \
      cargo test --quiet --manifest-path "${work_dir}/Cargo.toml" \
        generated_rust_loads_exported_messagepack_and_queries_indexes

    CARGO_TARGET_DIR="${work_dir}/target-${edition}-binary" \
    BIN_DATA_DIR="${FIXTURE_ROOT}/out/server_data/binary" \
      cargo test --quiet --manifest-path "${work_dir}/Cargo.toml" \
        generated_rust_loads_exported_binary_and_queries_indexes

    sed -i.bak "s/edition = \"${edition}\"/edition = \"2024\"/" "${work_dir}/Cargo.toml"
    rm -f "${work_dir}/Cargo.toml.bak"
  done
}

run_typescript_harness() {
  require_command node
  select_tsc_command
  local work_dir="${TMP_ROOT}/typescript"
  mkdir -p "${work_dir}"

  log "TypeScript harness: JSON / binary"
  cp "${SCRIPT_DIR}/typescript/runtime_check.ts" "${work_dir}/runtime_check.ts"
  cp "${SCRIPT_DIR}/typescript/tsconfig.json" "${work_dir}/tsconfig.json"
  cp "${FIXTURE_ROOT}/out/client_code/typescript/reference_game_config.ts" \
    "${work_dir}/generated.ts"
  cp "${REPO_ROOT}/runtime/typescript/reader_core/src/runtime_reader.ts" \
    "${work_dir}/runtime_reader_core.ts"
  cp "${REPO_ROOT}/runtime/typescript/binary_reader/src/runtime_loader.ts" \
    "${work_dir}/runtime_loader_binary.ts"
  cp "${REPO_ROOT}/runtime/typescript/json_reader/src/runtime_loader.ts" \
    "${work_dir}/runtime_loader_json.ts"
  cp "${REPO_ROOT}/runtime/typescript/loaders/node-fs/src/runtime_loader.ts" \
    "${work_dir}/runtime_loader_node_fs.ts"
  cp "${REPO_ROOT}/runtime/typescript/messagepack_reader/src/runtime_loader.ts" \
    "${work_dir}/runtime_loader_messagepack.ts"

  "${TSC_CMD[@]}" --project "${work_dir}/tsconfig.json"

  mkdir -p "${work_dir}/node_modules/@config-forge/runtime-reader-core"
  printf '{"type":"commonjs","main":"index.js"}\n' \
    > "${work_dir}/node_modules/@config-forge/runtime-reader-core/package.json"
  printf 'module.exports = require("../../../dist/runtime_reader_core.js");\n' \
    > "${work_dir}/node_modules/@config-forge/runtime-reader-core/index.js"

  DATA_DIR="${FIXTURE_ROOT}/out/client_data/json" \
    I18N_MO="${FIXTURE_ROOT}/i18n/zh_CN/reference_game_config.mo" \
    node "${work_dir}/dist/runtime_check.js"
  BIN_DATA_DIR="${FIXTURE_ROOT}/out/client_data/binary" \
    I18N_MO="${FIXTURE_ROOT}/i18n/zh_CN/reference_game_config.mo" \
    node "${work_dir}/dist/runtime_check.js"
}

run_cpp_harness() {
  require_command cmake
  local work_dir="${TMP_ROOT}/cpp"
  mkdir -p "${work_dir}"

  log "C++ harness: JSON / binary"
  cat > "${work_dir}/CMakeLists.txt" <<EOF
cmake_minimum_required(VERSION 3.16)
project(ConfigForgeStandardCppHarness LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CONFIG_FORGE_RUNTIME_WITH_RAPIDJSON_READER ON CACHE BOOL "" FORCE)
set(CONFIG_FORGE_RUNTIME_FETCH_DEPS ON CACHE BOOL "" FORCE)
add_subdirectory("${REPO_ROOT}/runtime/cpp" config_forge_runtime)

add_executable(standard_runtime_check_json
  "${SCRIPT_DIR}/cpp/runtime_check.cpp"
  "${FIXTURE_ROOT}/out/client_code/cpp/reference_game_config.cpp")
target_include_directories(standard_runtime_check_json PRIVATE
  "${FIXTURE_ROOT}/out/client_code/cpp")
target_link_libraries(standard_runtime_check_json PRIVATE config_forge_runtime::rapidjson_reader)

add_executable(standard_runtime_check_binary
  "${SCRIPT_DIR}/cpp/runtime_check.cpp"
  "${FIXTURE_ROOT}/out/client_code/cpp/reference_game_config.cpp")
target_include_directories(standard_runtime_check_binary PRIVATE
  "${FIXTURE_ROOT}/out/client_code/cpp")
target_compile_definitions(standard_runtime_check_binary PRIVATE CONFIG_FORGE_USE_BINARY_RUNTIME_READER)
target_link_libraries(standard_runtime_check_binary PRIVATE config_forge_runtime::binary_reader)
EOF

  cmake -S "${work_dir}" -B "${work_dir}/build"
  cmake --build "${work_dir}/build"
  DATA_DIR="${FIXTURE_ROOT}/out/client_data/json" \
    I18N_MO="${FIXTURE_ROOT}/i18n/zh_CN/reference_game_config.mo" \
    "${work_dir}/build/standard_runtime_check_json"
  DATA_DIR="${FIXTURE_ROOT}/out/client_data/binary" \
    I18N_MO="${FIXTURE_ROOT}/i18n/zh_CN/reference_game_config.mo" \
    "${work_dir}/build/standard_runtime_check_binary"
}

if [[ $# -eq 0 ]]; then
  TARGETS=(rust typescript cpp)
else
  TARGETS=("$@")
fi

for target in "${TARGETS[@]}"; do
  case "${target}" in
    rust) run_rust_harness ;;
    ts|typescript) run_typescript_harness ;;
    cpp|c++) run_cpp_harness ;;
    *)
      echo "Unknown harness target: ${target} (expected: rust, typescript, cpp)" >&2
      exit 1
      ;;
  esac
done

log "standard_project harness checks passed"
