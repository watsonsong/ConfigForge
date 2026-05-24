"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.MessagePackValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.decodeMessagePackValue = decodeMessagePackValue;
exports.createMessagePackRuntimeDecoder = createMessagePackRuntimeDecoder;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
exports.MessagePackValueReader = runtime_reader_core_1.RuntimeValueReader;
function decodeMessagePackValue(bytes, decode) {
    return new exports.MessagePackValueReader(decode(bytes));
}
function createMessagePackRuntimeDecoder(decode) {
    return (bytes) => decodeMessagePackValue(bytes, decode);
}
