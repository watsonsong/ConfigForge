// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.binary;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public class ConfigForgeBinaryRuntimeDataLoader {
    private final Path root;
    private int openCount;

    public ConfigForgeBinaryRuntimeDataLoader(Path root) {
        this.root = root;
    }

    public int getOpenCount() {
        return openCount;
    }

    public ConfigForgeBinaryRuntimeDataReader openReader(String path) {
        openCount++;
        Path resourcePath = root.resolve(path + ".bin");
        try {
            return new ConfigForgeBinaryRuntimeDataReader(Files.readAllBytes(resourcePath));
        } catch (IOException error) {
            throw new ConfigForgeRuntimeException(
                ConfigForgeRuntimeErrorKind.Io,
                "failed to read runtime data: " + path,
                resourcePath.toString(),
                error);
        }
    }
}
