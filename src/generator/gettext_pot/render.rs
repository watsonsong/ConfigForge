// Copyright (c) 2026 ConfigForge contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use super::{TextExtractionEntry, options::GettextPotOptions};

pub(super) fn render_pot(entries: &[TextExtractionEntry], options: &GettextPotOptions) -> String {
    let entries = if options.deduplicate {
        deduplicate_entries(entries)
    } else {
        entries.to_vec()
    };
    let mut output = String::new();
    output.push_str("msgid \"\"\n");
    output.push_str("msgstr \"\"\n");
    let project_id = options.domain.as_deref().unwrap_or("messages");
    output.push_str(&format!(
        "\"Project-Id-Version: {}\\n\"\n",
        escape_po(project_id)
    ));
    output.push_str("\"Content-Type: text/plain; charset=UTF-8\\n\"\n");
    output.push_str("\"Content-Transfer-Encoding: 8bit\\n\"\n\n");

    for entry in entries {
        for comment in &entry.translator_comments {
            output.push_str(&format!("#. {}\n", comment));
        }
        if let Some(source_reference) = &entry.source_reference {
            output.push_str(&format!("#: {}\n", source_reference));
        }
        if let Some(context) = &entry.msgctxt {
            output.push_str(&format!("msgctxt \"{}\"\n", escape_po(context)));
        }
        output.push_str(&format!("msgid \"{}\"\n", escape_po(&entry.msgid)));
        output.push_str("msgstr \"\"\n\n");
    }
    output
}

pub(super) fn deduplicate_entries(entries: &[TextExtractionEntry]) -> Vec<TextExtractionEntry> {
    let mut map: BTreeMap<(Option<String>, String), TextExtractionEntry> = BTreeMap::new();
    for entry in entries {
        let key = (entry.msgctxt.clone(), entry.msgid.clone());
        if let Some(existing) = map.get_mut(&key) {
            merge_unique(
                &mut existing.translator_comments,
                &entry.translator_comments,
            );
            if existing.source_reference != entry.source_reference {
                match (&existing.source_reference, &entry.source_reference) {
                    (Some(left), Some(right)) => {
                        existing.source_reference = Some(format!("{left} {right}"));
                    }
                    (None, Some(right)) => existing.source_reference = Some(right.clone()),
                    _ => {}
                }
            }
        } else {
            map.insert(key, entry.clone());
        }
    }
    map.into_values().collect()
}

fn merge_unique(target: &mut Vec<String>, source: &[String]) {
    for value in source {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

pub(super) fn escape_po(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\x{:02x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}
