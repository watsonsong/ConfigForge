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
        Path resourcePath = root.resolve(path + ".msgpack");
        try {
            return new ConfigForgeMessagePackRuntimeDataReader(
                ConfigForgeMessagePackParser.parse(Files.readAllBytes(resourcePath)));
        } catch (IOException error) {
            throw new ConfigForgeRuntimeException(
                ConfigForgeRuntimeErrorKind.Io,
                "failed to read runtime data: " + path,
                resourcePath.toString(),
                error);
        }
    }
}
