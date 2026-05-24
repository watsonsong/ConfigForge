"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.JsonValueReader = exports.SUPPORTED_CONFIG_FORGE_READER_CONTRACT = void 0;
exports.decodeJsonValue = decodeJsonValue;
const runtime_reader_core_1 = require("@config-forge/runtime-reader-core");
Object.defineProperty(exports, "SUPPORTED_CONFIG_FORGE_READER_CONTRACT", { enumerable: true, get: function () { return runtime_reader_core_1.SUPPORTED_CONFIG_FORGE_READER_CONTRACT; } });
class JsonValueReader extends runtime_reader_core_1.RuntimeValueReader {
}
exports.JsonValueReader = JsonValueReader;
function decodeJsonValue(bytes) {
    const text = typeof bytes === "string" ? bytes : new TextDecoder().decode(bytes);
    return new JsonValueReader(JSON.parse(text));
}
