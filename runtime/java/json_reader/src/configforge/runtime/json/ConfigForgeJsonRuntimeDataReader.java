// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.json;

import java.util.List;
import java.util.Map;
import java.util.Stack;

public class ConfigForgeJsonRuntimeDataReader {
    public static final String TEMPLATE_VERSION = "0.1.0";
    public static final String SUPPORTED_CONFIG_FORGE_READER_CONTRACT = "^1.0";
    public static final String DISTRIBUTION_MODE = "copyable-template";

    private final Stack<Object> stack = new Stack<Object>();

    public ConfigForgeJsonRuntimeDataReader(Object root) {
        stack.push(root);
    }

    public void enterRequiredField(String name) {
        Object current = stack.peek();
        if (current instanceof Map && ((Map<?, ?>) current).containsKey(name) && ((Map<?, ?>) current).get(name) != null) {
            stack.push(((Map<?, ?>) current).get(name));
        } else {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.NotFound, "missing required field " + name);
        }
    }

    public boolean enterOptionalField(String name) {
        Object current = stack.peek();
        if (current instanceof Map && ((Map<?, ?>) current).containsKey(name) && ((Map<?, ?>) current).get(name) != null) {
            stack.push(((Map<?, ?>) current).get(name));
            return true;
        }
        return false;
    }

    public void exitField() {
        stack.pop();
    }

    public Long schemaRevision() {
        return null;
    }

    public int readRowCount() {
        Object current = stack.peek();
        if (current instanceof List) {
            return ((List<?>) current).size();
        }
        if (current instanceof Map && ((Map<?, ?>) current).get("rows") instanceof List) {
            return ((List<?>) ((Map<?, ?>) current).get("rows")).size();
        }
        return 1;
    }

    public void enterRow(int index) {
        Map<?, ?> current = (Map<?, ?>) stack.peek();
        stack.push(((List<?>) current.get("rows")).get(index));
    }

    public void exitRow() {
        stack.pop();
    }

    public int readListLength() {
        Object current = stack.peek();
        if (!(current instanceof List)) {
            throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "expected JSON array, got " + typeName(current));
        }
        return ((List<?>) current).size();
    }

    public void enterListElement(int index) {
        stack.push(((List<?>) stack.peek()).get(index));
    }

    public void exitListElement() {
        stack.pop();
    }

    public void beginRecord(int optionalSlotCount) {
    }

    public void endRecord() {
    }

    public int readEnumIndex(String[] variants) {
        String value = readString();
        for (int index = 0; index < variants.length; index++) {
            if (variants[index].equals(value)) {
                return index;
            }
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "unknown enum value: " + value);
    }

    public void ensureExhausted() {
    }

    public long readInt() {
        Object value = stack.peek();
        if (value instanceof Number) {
            return ((Number) value).longValue();
        }
        if (value instanceof String) {
            return Long.parseLong((String) value);
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "expected int-like value, got " + typeName(value));
    }

    public double readFloat() {
        Object value = stack.peek();
        if (value instanceof Number) {
            return ((Number) value).doubleValue();
        }
        if (value instanceof String) {
            double parsed = Double.parseDouble((String) value);
            if (!Double.isFinite(parsed)) {
                throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "expected finite float-like value");
            }
            return parsed;
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "expected finite float-like value, got " + typeName(value));
    }

    public boolean readBool() {
        Object value = stack.peek();
        if (value instanceof Boolean) {
            return ((Boolean) value).booleanValue();
        }
        if (value instanceof String) {
            String text = ((String) value).trim();
            if (text.equals("1")) {
                return true;
            }
            if (text.equals("0")) {
                return false;
            }
            if (text.equals("true")) {
                return true;
            }
            if (text.equals("false")) {
                return false;
            }
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.Decode, "expected bool-like value, got " + typeName(value));
    }

    public String readString() {
        Object value = stack.peek();
        if (value instanceof String) {
            return (String) value;
        }
        throw new ConfigForgeRuntimeException(ConfigForgeRuntimeErrorKind.TypeMismatch, "expected string value, got " + typeName(value));
    }

    private static String typeName(Object value) {
        return value == null ? "null" : value.getClass().getSimpleName();
    }
}
