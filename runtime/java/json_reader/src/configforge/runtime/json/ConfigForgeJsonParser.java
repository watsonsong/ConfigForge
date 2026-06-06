// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

package configforge.runtime.json;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

public final class ConfigForgeJsonParser {
    private final String text;
    private int index;

    private ConfigForgeJsonParser(String text) {
        this.text = text;
    }

    public static Object parse(String text) {
        return new ConfigForgeJsonParser(text).readValue();
    }

    private Object readValue() {
        skipWhitespace();
        char ch = text.charAt(index);
        if (ch == '{') {
            return readObject();
        }
        if (ch == '[') {
            return readArray();
        }
        if (ch == '"') {
            return readStringValue();
        }
        if (text.startsWith("true", index)) {
            index += 4;
            return Boolean.TRUE;
        }
        if (text.startsWith("false", index)) {
            index += 5;
            return Boolean.FALSE;
        }
        if (text.startsWith("null", index)) {
            index += 4;
            return null;
        }
        return readNumber();
    }

    private Map<String, Object> readObject() {
        index++;
        skipWhitespace();
        Map<String, Object> result = new HashMap<String, Object>();
        if (text.charAt(index) == '}') {
            index++;
            return result;
        }
        while (true) {
            String key = readStringValue();
            skipWhitespace();
            expect(':');
            result.put(key, readValue());
            skipWhitespace();
            if (text.charAt(index) == '}') {
                index++;
                return result;
            }
            expect(',');
        }
    }

    private List<Object> readArray() {
        index++;
        skipWhitespace();
        List<Object> result = new ArrayList<Object>();
        if (text.charAt(index) == ']') {
            index++;
            return result;
        }
        while (true) {
            result.add(readValue());
            skipWhitespace();
            if (text.charAt(index) == ']') {
                index++;
                return result;
            }
            expect(',');
        }
    }

    private String readStringValue() {
        expect('"');
        StringBuilder builder = new StringBuilder();
        while (true) {
            char ch = text.charAt(index++);
            if (ch == '"') {
                return builder.toString();
            }
            if (ch == '\\') {
                char escaped = text.charAt(index++);
                switch (escaped) {
                    case '"': builder.append('"'); break;
                    case '\\': builder.append('\\'); break;
                    case '/': builder.append('/'); break;
                    case 'b': builder.append('\b'); break;
                    case 'f': builder.append('\f'); break;
                    case 'n': builder.append('\n'); break;
                    case 'r': builder.append('\r'); break;
                    case 't': builder.append('\t'); break;
                    case 'u':
                        builder.append((char) Integer.parseInt(text.substring(index, index + 4), 16));
                        index += 4;
                        break;
                    default:
                        throw new IllegalArgumentException("invalid escape: " + escaped);
                }
            } else {
                builder.append(ch);
            }
        }
    }

    private String readNumber() {
        int start = index;
        while (index < text.length() && "-+0123456789.eE".indexOf(text.charAt(index)) >= 0) {
            index++;
        }
        return text.substring(start, index);
    }

    private void expect(char expected) {
        skipWhitespace();
        if (text.charAt(index) != expected) {
            throw new IllegalArgumentException("expected " + expected + " at " + index);
        }
        index++;
        skipWhitespace();
    }

    private void skipWhitespace() {
        while (index < text.length() && Character.isWhitespace(text.charAt(index))) {
            index++;
        }
    }
}
