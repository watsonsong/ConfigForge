// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.json;

public class ConfigForgeRuntimeException extends RuntimeException {
    private final ConfigForgeRuntimeErrorKind kind;
    private final String path;

    public ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind kind, String message) {
        super(message);
        this.kind = kind;
        this.path = null;
    }

    public ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind kind, String message, Throwable cause) {
        super(message, cause);
        this.kind = kind;
        this.path = null;
    }

    public ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind kind, String message, String path, Throwable cause) {
        super(message, cause);
        this.kind = kind;
        this.path = path;
    }

    public ConfigForgeRuntimeErrorKind kind() { return kind; }
    public String code() {
        switch (kind) {
            case Io: return "CF_RT_IO";
            case Decode: return "CF_RT_DECODE";
            case SchemaMismatch: return "CF_RT_SCHEMA_MISMATCH";
            case TypeMismatch: return "CF_RT_TYPE_MISMATCH";
            case NotFound: return "CF_RT_NOT_FOUND";
            case Unsupported: return "CF_RT_UNSUPPORTED";
            case InvalidPath: return "CF_RT_INVALID_PATH";
            default: return "CF_RT_INTERNAL";
        }
    }
    public String path() { return path; }
}
