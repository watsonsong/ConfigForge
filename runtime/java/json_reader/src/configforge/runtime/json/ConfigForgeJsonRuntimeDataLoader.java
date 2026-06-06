// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.json;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

public class ConfigForgeJsonRuntimeDataLoader {
    private final Path root;
    private int openCount;

    public ConfigForgeJsonRuntimeDataLoader(Path root) {
        this.root = root;
    }

    public int getOpenCount() {
        return openCount;
    }

    public ConfigForgeJsonRuntimeDataReader openReader(String path) {
        openCount++;
        try {
            String text = new String(Files.readAllBytes(root.resolve(path + ".json")), StandardCharsets.UTF_8);
            return new ConfigForgeJsonRuntimeDataReader(ConfigForgeJsonParser.parse(text));
        } catch (IOException error) {
            throw new RuntimeException(error);
        }
    }
}
