"use strict";
// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0
Object.defineProperty(exports, "__esModule", { value: true });
exports.MessagePackValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.decodeMessagePackValue = decodeMessagePackValue;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
class MessagePackValueReader extends runtime_reader_core_1.RuntimeValueReader {
}
exports.MessagePackValueReader = MessagePackValueReader;
function decodeMessagePackValue(bytes, decode) {
    return new MessagePackValueReader(decode(bytes));
}
