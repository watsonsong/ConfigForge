"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.JsonValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.decodeJsonValue = decodeJsonValue;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
exports.JsonValueReader = runtime_reader_core_1.RuntimeValueReader;
const utf8Decoder = new TextDecoder();
function decodeJsonValue(bytes) {
    const text = typeof bytes === "string" ? bytes : utf8Decoder.decode(bytes);
    return new exports.JsonValueReader(JSON.parse(text));
}
