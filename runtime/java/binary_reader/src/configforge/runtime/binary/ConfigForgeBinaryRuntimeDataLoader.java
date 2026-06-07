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
        try {
            return new ConfigForgeBinaryRuntimeDataReader(Files.readAllBytes(root.resolve(path + ".bin")));
        } catch (IOException error) {
            throw new RuntimeException(error);
        }
    }
}
