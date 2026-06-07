// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.messagepack;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public class ConfigForgeMessagePackRuntimeDataLoader {
    private final Path root;
    private int openCount;

    public ConfigForgeMessagePackRuntimeDataLoader(Path root) {
        this.root = root;
    }

    public int getOpenCount() {
        return openCount;
    }

    public ConfigForgeMessagePackRuntimeDataReader openReader(String path) {
        openCount++;
        try {
            return new ConfigForgeMessagePackRuntimeDataReader(
                ConfigForgeMessagePackParser.parse(Files.readAllBytes(root.resolve(path + ".msgpack"))));
        } catch (IOException error) {
            throw new RuntimeException(error);
        }
    }
}
