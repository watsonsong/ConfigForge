// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.messagepack;

import java.util.List;
import java.util.Map;
import java.util.Stack;

public class ConfigForgeMessagePackRuntimeDataReader {
    private final Stack<Object> stack = new Stack<Object>();

    public ConfigForgeMessagePackRuntimeDataReader(Object root) {
        stack.push(root);
    }

    public void enterRequiredField(String name) {
        Object current = stack.peek();
        if (current instanceof Map && ((Map<?, ?>) current).containsKey(name)) {
            stack.push(((Map<?, ?>) current).get(name));
        } else {
            stack.push(null);
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

    public int readRowCount() {
        Object current = stack.peek();
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
            throw new IllegalStateException("expected MessagePack array, got " + typeName(current));
        }
        return ((List<?>) current).size();
    }

    public void enterListElement(int index) {
        stack.push(((List<?>) stack.peek()).get(index));
    }

    public void exitListElement() {
        stack.pop();
    }

    public long readInt() {
        Object value = stack.peek();
        if (value instanceof Number) {
            return ((Number) value).longValue();
        }
        if (value instanceof String) {
            return Long.parseLong((String) value);
        }
        return 0L;
    }

    public double readFloat() {
        Object value = stack.peek();
        if (value instanceof Number) {
            return ((Number) value).doubleValue();
        }
        if (value instanceof String) {
            return Double.parseDouble((String) value);
        }
        return 0.0;
    }

    public boolean readBool() {
        Object value = stack.peek();
        if (value instanceof Boolean) {
            return ((Boolean) value).booleanValue();
        }
        if (value instanceof Number) {
            return ((Number) value).longValue() != 0L;
        }
        if (value instanceof String) {
            String text = ((String) value).trim();
            if (text.equals("1")) {
                return true;
            }
            if (text.equals("0")) {
                return false;
            }
            return Boolean.parseBoolean(text);
        }
        return false;
    }

    public String readString() {
        Object value = stack.peek();
        if (value instanceof String) {
            return (String) value;
        }
        if (value instanceof Number || value instanceof Boolean) {
            return String.valueOf(value);
        }
        return "";
    }

    private static String typeName(Object value) {
        return value == null ? "nil" : value.getClass().getSimpleName();
    }
}
