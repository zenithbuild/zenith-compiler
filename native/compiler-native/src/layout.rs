use napi_derive::napi;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutMetadataForProcessing {
    pub name: String,
    pub html: String,
    pub scripts: Vec<String>,
    pub styles: Vec<String>,
}

#[napi]
pub fn process_layout_native(source: String, layout_json: String, props_json: String) -> String {
    let layout: LayoutMetadataForProcessing = serde_json::from_str(&layout_json).unwrap();
    let initial_props: HashMap<String, String> = serde_json::from_str(&props_json).unwrap();

    // 1. Extract scripts and styles from the page source
    let mut page_scripts = Vec::new();
    let mut page_styles = Vec::new();
    let mut is_typescript = false;

    let script_re = Regex::new(r##"(?is)<script\b([^>]*)>([\s\S]*?)</script>"##).unwrap();
    for cap in script_re.captures_iter(&source) {
        let attrs = &cap[1];
        let content = &cap[2];
        if attrs.contains("lang=\"ts\"") || attrs.contains("setup=\"ts\"") {
            is_typescript = true;
        }
        page_scripts.push(content.trim().to_string());
    }

    let style_re = Regex::new(r##"(?is)<style[^>]*>([\s\S]*?)</style>"##).unwrap();
    for cap in style_re.captures_iter(&source) {
        page_styles.push(cap[1].trim().to_string());
    }

    // 2. Extract content from page source and parse props
    let layout_tag = &layout.name;
    let layout_re_pattern = format!(
        r"(?i)<{}\b([^>]*)>(?:([\s\S]*?)</{}>)?",
        layout_tag, layout_tag
    );
    let layout_re = Regex::new(&layout_re_pattern).unwrap();

    let mut page_html = String::new();
    let mut layout_props_str = String::new();

    if let Some(cap) = layout_re.captures(&source) {
        layout_props_str = cap.get(1).map_or("", |m| m.as_str()).to_string();
        page_html = cap.get(2).map_or("", |m| m.as_str()).to_string();
    } else {
        // Fallback: assume everything minus script/style is content
        page_html = script_re.replace_all(&source, "").to_string();
        page_html = style_re.replace_all(&page_html, "").trim().to_string();
    }

    // 3. Parse props from the tag
    let mut merged_props = initial_props;
    let attr_re = Regex::new(r##"([a-zA-Z0-9-]+)=(?:\{([^}]+)\}|"([^"]*)"|'([^']*)')"##).unwrap();
    for cap in attr_re.captures_iter(&layout_props_str) {
        let name = cap[1].to_string();
        let value = cap
            .get(2)
            .or(cap.get(3))
            .or(cap.get(4))
            .map(|m| m.as_str())
            .unwrap_or("");
        if name != "props" {
            merged_props.insert(name, value.to_string());
        }
    }

    // 4. Merge Scripts with Prop Injection
    let mut prop_decls = Vec::new();
    for (key, value) in &merged_props {
        let is_expression = layout_props_str.contains(&format!("{}={{{}}}", key, value));
        if is_expression {
            prop_decls.push(format!("const {} = {};", key, value));
        } else {
            let formatted_value = if !value.starts_with('\'') && !value.starts_with('"') {
                format!("'{}'", value)
            } else {
                value.clone()
            };
            prop_decls.push(format!("const {} = {};", key, formatted_value));
        }
    }

    let all_scripts_raw = format!(
        "{}\n\n{}\n\n{}",
        prop_decls.join("\n"),
        layout.scripts.join("\n\n"),
        page_scripts.join("\n\n")
    );

    let merged_scripts = deduplicate_imports(&all_scripts_raw);

    // 5. Merge Styles
    let merged_styles = layout
        .styles
        .iter()
        .chain(page_styles.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n\n");

    // 6. Inline HTML into layout slot
    let slot_re = Regex::new(r##"(?i)<Slot\s*/>|<slot\s*>[\s\S]*?</slot>"##).unwrap();
    let finalized_html = slot_re.replace_all(&layout.html, &page_html);

    // 7. Reconstruct the full .zen source
    let prop_names = merged_props.keys().cloned().collect::<Vec<_>>().join(",");
    let script_tag = format!(
        "<script setup{}{}>",
        if is_typescript { "=\"ts\"" } else { "" },
        if !prop_names.is_empty() {
            format!(" props=\"{}\"", prop_names)
        } else {
            String::new()
        }
    );

    format!(
        "{}\n{}\n</script>\n\n{}\n\n<style>\n{}\n</style>",
        script_tag, merged_scripts, finalized_html, merged_styles
    )
    .trim()
    .to_string()
}

fn deduplicate_imports(script: &str) -> String {
    let import_re =
        Regex::new(r##"(?m)^import\s+.+\s+from\s+['\"][^'\"]+['\"]\s*;?\s*$"##).unwrap();
    let mut imports = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let without_imports = import_re.replace_all(script, |caps: &regex::Captures| {
        let matched = caps[0].trim();
        let normalized = matched.replace('"', "'");
        let normalized = if !normalized.ends_with(';') {
            format!("{};", normalized)
        } else {
            normalized
        };

        if !seen.contains(&normalized) {
            seen.insert(normalized.clone());
            imports.push(normalized);
        }
        String::new()
    });

    let cleaned_body = without_imports.trim();
    if imports.is_empty() {
        cleaned_body.to_string()
    } else {
        format!("{}\n\n{}", imports.join("\n"), cleaned_body)
    }
}
