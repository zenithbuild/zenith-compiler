//! Discovery Module for Zenith Compiler
//!
//! Port of componentDiscovery.ts and layouts.ts to Rust.
//! Recursively scans directories for .zen files and extracts metadata.

#[cfg(feature = "napi")]
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::parse::{is_component_tag, parse_script, parse_template};
use crate::validate::{AttributeValue, ExpressionIR, SourceLocation, TemplateNode};

// ═══════════════════════════════════════════════════════════════════════════════
// METADATA TYPES
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotDefinition {
    pub name: Option<String>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentMetadata {
    pub name: String,
    pub path: String,
    pub template: String,
    pub nodes: Vec<TemplateNode>,
    pub expressions: Vec<ExpressionIR>,
    pub slots: Vec<SlotDefinition>,
    pub props: Vec<String>,
    pub states: HashMap<String, String>,
    pub locals: Vec<String>,
    pub styles: Vec<String>,
    pub script: Option<String>,
    pub script_attributes: Option<HashMap<String, String>>,
    pub has_script: bool,
    pub has_styles: bool,
}

// LayoutMetadata removed - layouts are now just components

// ═══════════════════════════════════════════════════════════════════════════════
// COMPONENT DISCOVERY
// ═══════════════════════════════════════════════════════════════════════════════

/// Discover all components in a directory
#[cfg_attr(feature = "napi", napi)]
pub fn discover_components_native(base_dir: String) -> serde_json::Value {
    let mut components = HashMap::new();
    let path = Path::new(&base_dir);

    if !path.exists() {
        return serde_json::to_value(components).unwrap_or(serde_json::Value::Null);
    }

    let files = find_zen_files(path);

    for file_path in files {
        match parse_component_file(&file_path) {
            Ok(metadata) => {
                components.insert(metadata.name.clone(), metadata);
            }
            Err(e) => {
                eprintln!("[Zenith] Failed to parse component {:?}: {}", file_path, e);
                // Continue despite errors in one component
            }
        }
    }

    serde_json::to_value(components).unwrap_or(serde_json::Value::Null)
}

/// Recursively find all .zen files in a directory
fn find_zen_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir).follow_links(true) {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "zen" {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }
    }

    files
}

/// Parse a component file and extract metadata
fn parse_component_file(file_path: &Path) -> Result<ComponentMetadata, String> {
    let source =
        fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let path_str = file_path.to_string_lossy().to_string();

    // Parse template
    let template_ir = parse_template(&source, &path_str).map_err(|e| e.message)?;

    // Parse script
    let script_ir = parse_script(&source);

    // Extract component name from filename
    let name = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid filename".to_string())?;

    // Extract slots
    let slots = extract_slots(&template_ir.nodes);

    // Extract props & states from script
    let mut props = Vec::new();
    let mut states = HashMap::new();
    let mut locals = Vec::new();

    if let Some(ref s) = script_ir {
        // 1. Attributes-based props (legacy)
        if let Some(p_attr) = s.attributes.get("props") {
            props.extend(p_attr.split(',').map(|s| s.trim().to_string()));
        }

        // 2. Syntax-based props (prop x)
        props.extend(extract_props_from_script(&s.raw));

        // 3. Interface-based props (interface Props { name: type; ... })
        props.extend(extract_props_from_interface(&s.raw));

        // 4. Syntax-based state (state x = y)
        states = extract_state_from_script(&s.raw);

        // 5. Capture all top-level locals (const, let, var, function)
        let captured_locals = extract_locals_from_script(&s.raw);
        locals.extend(captured_locals);
    }

    eprintln!("[Zenith DISCOVERY] Component: {}", name);
    if let Some(ref s) = script_ir {
        eprintln!("[Zenith DISCOVERY] Found script ({} chars)", s.raw.len());
    } else {
        eprintln!("[Zenith DISCOVERY] No script found");
    }
    eprintln!("[Zenith DISCOVERY] Extracted Props: {:?}", props);
    eprintln!("[Zenith DISCOVERY] Extracted States: {:?}", states);
    eprintln!("[Zenith DISCOVERY] Extracted Locals: {:?}", locals);

    // Extract CSS (Simple regex extraction for now, similar to original)
    // In a full implementation, this might reuse style parsing logic from lib.rs/index.ts
    // For now we'll do a quick pass or rely on what parse_template gives if it included styles?
    // parse_template strips styles. So we need to extract them separately or reuse logic.
    // Let's do a simple regex extraction here as we did in parse.rs but expose it.
    // Actually parse.rs strips them. Let's add a helper or just regex it here.
    let styles = extract_styles_native(source.clone());

    Ok(ComponentMetadata {
        name,
        path: path_str,
        template: template_ir.raw,
        nodes: template_ir.nodes,
        expressions: template_ir.expressions,
        slots,
        props,
        states,
        locals,
        styles: styles.clone(),
        script: script_ir.clone().map(|s| s.raw),
        script_attributes: script_ir.clone().map(|s| s.attributes),
        has_script: script_ir.is_some(),
        has_styles: !styles.is_empty(),
    })
}

#[cfg_attr(feature = "napi", napi)]
pub fn extract_styles_native(source: String) -> Vec<String> {
    let re = regex::Regex::new(r"(?is)<style[^>]*>([\s\S]*?)</style>").unwrap();
    re.captures_iter(&source)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().trim().to_string()))
        .collect()
}

#[cfg_attr(feature = "napi", napi)]
pub fn extract_page_bindings_native(script: String) -> Vec<String> {
    let mut bindings = Vec::new();
    let re = regex::Regex::new(r"state\s+([a-zA-Z_$][a-zA-Z0-9_$]*)(?:\s*=\s*([^;\n]+))?").unwrap();
    for cap in re.captures_iter(&script) {
        if let Some(m) = cap.get(1) {
            bindings.push(m.as_str().to_string());
        }
    }
    bindings
}

#[cfg_attr(feature = "napi", napi)]
pub fn extract_page_props_native(script: String) -> Vec<String> {
    let mut props = Vec::new();
    let re = regex::Regex::new(r"(?:^|[;{}\n])\s*prop\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    for cap in re.captures_iter(&script) {
        if let Some(m) = cap.get(1) {
            props.push(m.as_str().to_string());
        }
    }
    props
}

/// Extract slot definitions from template nodes
fn extract_slots(nodes: &[TemplateNode]) -> Vec<SlotDefinition> {
    let mut slots = Vec::new();

    fn traverse(node: &TemplateNode, slots: &mut Vec<SlotDefinition>) {
        match node {
            TemplateNode::Element(el) => {
                if el.tag == "slot" {
                    let mut name = None;
                    for attr in &el.attributes {
                        if attr.name == "name" {
                            if let AttributeValue::Static(val) = &attr.value {
                                name = Some(val.clone());
                            }
                        }
                    }
                    slots.push(SlotDefinition {
                        name,
                        location: el.location.clone(),
                    });
                }

                for child in &el.children {
                    traverse(child, slots);
                }
            }
            TemplateNode::Component(comp) => {
                for child in &comp.children {
                    traverse(child, slots);
                }
            }
            TemplateNode::ConditionalFragment(cf) => {
                for child in &cf.consequent {
                    traverse(child, slots);
                }
                for child in &cf.alternate {
                    traverse(child, slots);
                }
            }
            TemplateNode::OptionalFragment(of) => {
                for child in &of.fragment {
                    traverse(child, slots);
                }
            }
            TemplateNode::LoopFragment(lf) => {
                for child in &lf.body {
                    traverse(child, slots);
                }
            }
            _ => {}
        }
    }

    for node in nodes {
        traverse(node, &mut slots);
    }

    slots
}
// ═══════════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS FOR SCRIPT PARSING
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_props_from_script(script: &str) -> Vec<String> {
    let mut props = Vec::new();
    // Support both 'prop name = ...' and 'prop name'
    let re = regex::Regex::new(r"(?m)^\s*prop\s+([a-zA-Z_$][a-zA-Z0-9_$]*)(?:\s*=\s*([^;\n]+))?")
        .unwrap();
    for cap in re.captures_iter(script) {
        if let Some(m) = cap.get(1) {
            props.push(m.as_str().to_string());
        }
    }
    props
}

fn extract_state_from_script(script: &str) -> HashMap<String, String> {
    let mut states = HashMap::new();
    let re = regex::Regex::new(r"(?m)^\s*state\s+([a-zA-Z_$][a-zA-Z0-9_$]*)(?:\s*=\s*([^;\n]+))?")
        .unwrap();

    for cap in re.captures_iter(script) {
        if let Some(name) = cap.get(1) {
            let val = cap
                .get(2)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_else(|| "undefined".to_string());
            states.insert(name.as_str().to_string(), val);
        }
    }
    states
}

fn extract_locals_from_script(script: &str) -> Vec<String> {
    let mut locals = Vec::new();
    // Match const|let|var|function followed by name
    let re = regex::Regex::new(r"(?m)^\s*(?:const|let|var|function)\s+([a-zA-Z_$][a-zA-Z0-9_$]*)")
        .unwrap();
    for cap in re.captures_iter(script) {
        if let Some(m) = cap.get(1) {
            locals.push(m.as_str().to_string());
        }
    }
    locals
}

/// Extract props from TypeScript interface Props { ... } syntax.
/// Matches patterns like:
/// - interface Props { title: string; description: string; }
/// - interface Props {\n    title: string;\n    number: number;\n}
fn extract_props_from_interface(script: &str) -> Vec<String> {
    let mut props = Vec::new();

    // Match `interface Props { ... }` block
    // We use a regex to find the interface block, then parse internal properties
    let interface_re = regex::Regex::new(r"(?s)interface\s+Props\s*\{([^}]*)\}").unwrap();

    if let Some(cap) = interface_re.captures(script) {
        if let Some(body) = cap.get(1) {
            let body_str = body.as_str();
            // Match property definitions: name: type or name?: type
            let prop_re = regex::Regex::new(r"([a-zA-Z_$][a-zA-Z0-9_$]*)\s*\??\s*:").unwrap();
            for prop_cap in prop_re.captures_iter(body_str) {
                if let Some(m) = prop_cap.get(1) {
                    props.push(m.as_str().to_string());
                }
            }
        }
    }

    eprintln!(
        "[Zenith DISCOVERY interface] Extracted interface Props: {:?}",
        props
    );
    props
}
