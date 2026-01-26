//! Parse Module for Zenith Compiler
//!
//! Port of parseTemplate.ts and parseScript.ts to Rust.
//! Provides HTML5-compliant template parsing with expression extraction.

use html5ever::parse_document;
use html5ever::tendril::TendrilSink;
use lazy_static::lazy_static;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use napi_derive::napi;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::validate::{
    AttributeIR, CompilerError, ComponentNode, DoctypeNode, ElementNode, ExpressionIR,
    ExpressionNode, LoopContext, ScriptIR, SourceLocation, TemplateIR, TemplateNode, TextNode,
};

// ═══════════════════════════════════════════════════════════════════════════════
// SVG ATTRIBUTE CASE MAPPING
// ═══════════════════════════════════════════════════════════════════════════════

lazy_static! {
    /// SVG attribute case mapping - parse5/html5ever lowercases all attributes,
    /// but SVG requires specific casing for these attributes.
    static ref SVG_ATTR_CASE_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Core SVG attributes with camelCase
        m.insert("viewbox", "viewBox");
        m.insert("preserveaspectratio", "preserveAspectRatio");
        m.insert("basefrequency", "baseFrequency");
        m.insert("baseprofile", "baseProfile");
        m.insert("clippathunits", "clipPathUnits");
        m.insert("diffuseconstant", "diffuseConstant");
        m.insert("edgemode", "edgeMode");
        m.insert("filterunits", "filterUnits");
        m.insert("glyphref", "glyphRef");
        m.insert("gradienttransform", "gradientTransform");
        m.insert("gradientunits", "gradientUnits");
        m.insert("kernelmatrix", "kernelMatrix");
        m.insert("kernelunitlength", "kernelUnitLength");
        m.insert("keypoints", "keyPoints");
        m.insert("keysplines", "keySplines");
        m.insert("keytimes", "keyTimes");
        m.insert("lengthadjust", "lengthAdjust");
        m.insert("limitingconeangle", "limitingConeAngle");
        m.insert("markerheight", "markerHeight");
        m.insert("markerunits", "markerUnits");
        m.insert("markerwidth", "markerWidth");
        m.insert("maskcontentunits", "maskContentUnits");
        m.insert("maskunits", "maskUnits");
        m.insert("numoctaves", "numOctaves");
        m.insert("pathlength", "pathLength");
        m.insert("patterncontentunits", "patternContentUnits");
        m.insert("patterntransform", "patternTransform");
        m.insert("patternunits", "patternUnits");
        m.insert("pointsatx", "pointsAtX");
        m.insert("pointsaty", "pointsAtY");
        m.insert("pointsatz", "pointsAtZ");
        m.insert("primitiveunits", "primitiveUnits");
        m.insert("refx", "refX");
        m.insert("refy", "refY");
        m.insert("repeatcount", "repeatCount");
        m.insert("repeatdur", "repeatDur");
        m.insert("requiredextensions", "requiredExtensions");
        m.insert("requiredfeatures", "requiredFeatures");
        m.insert("specularconstant", "specularConstant");
        m.insert("specularexponent", "specularExponent");
        m.insert("spreadmethod", "spreadMethod");
        m.insert("startoffset", "startOffset");
        m.insert("stddeviation", "stdDeviation");
        m.insert("stitchtiles", "stitchTiles");
        m.insert("surfacescale", "surfaceScale");
        m.insert("systemlanguage", "systemLanguage");
        m.insert("tablevalues", "tableValues");
        m.insert("targetx", "targetX");
        m.insert("targety", "targetY");
        m.insert("textlength", "textLength");
        m.insert("xchannelselector", "xChannelSelector");
        m.insert("ychannelselector", "yChannelSelector");
        m.insert("zoomandpan", "zoomAndPan");
        m.insert("attributename", "attributeName");
        m.insert("attributetype", "attributeType");
        m.insert("calcmode", "calcMode");
        m.insert("clippath", "clipPath");
        m
    };

    /// SVG tags set
    static ref SVG_TAGS: std::collections::HashSet<&'static str> = {
        let mut s = std::collections::HashSet::new();
        s.insert("svg");
        s.insert("path");
        s.insert("circle");
        s.insert("ellipse");
        s.insert("line");
        s.insert("polyline");
        s.insert("polygon");
        s.insert("rect");
        s.insert("g");
        s.insert("defs");
        s.insert("use");
        s.insert("symbol");
        s.insert("clippath");
        s.insert("mask");
        s.insert("pattern");
        s.insert("marker");
        s.insert("lineargradient");
        s.insert("radialgradient");
        s.insert("stop");
        s.insert("filter");
        s.insert("feblend");
        s.insert("fecolormatrix");
        s.insert("fecomponenttransfer");
        s.insert("fecomposite");
        s.insert("feconvolvematrix");
        s.insert("fediffuselighting");
        s.insert("fedisplacementmap");
        s.insert("fedropshadow");
        s.insert("feflood");
        s.insert("fefunca");
        s.insert("fefuncb");
        s.insert("fefuncg");
        s.insert("fefuncr");
        s.insert("fegaussianblur");
        s.insert("feimage");
        s.insert("femerge");
        s.insert("femergenode");
        s.insert("femorphology");
        s.insert("feoffset");
        s.insert("fespecularlighting");
        s.insert("fetile");
        s.insert("feturbulence");
        s.insert("foreignobject");
        s.insert("image");
        s.insert("switch");
        s.insert("text");
        s.insert("tspan");
        s.insert("textpath");
        s.insert("title");
        s.insert("desc");
        s.insert("metadata");
        s.insert("a");
        s.insert("view");
        s.insert("animate");
        s.insert("animatemotion");
        s.insert("animatetransform");
        s.insert("set");
        s.insert("mpath");
        s
    };

    /// Expression placeholder pattern for normalization
    static ref EXPR_PLACEHOLDER_RE: Regex = Regex::new(r"__ZENITH_EXPR_(\d+)__").unwrap();

    /// Script block regex
    static ref SCRIPT_REGEX: Regex = Regex::new(r"(?is)<script\b([^>]*)>([\s\S]*?)</script>").unwrap();

    /// Attribute regex for parsing script attributes
    static ref ATTR_REGEX: Regex = Regex::new(r#"(?i)([a-z0-9-]+)(?:=(?:"([^"]*)"|'([^']*)'|([^>\s]+)))?"#).unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEMPLATE IR TYPES
// ═══════════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════════
// EXPRESSION ID GENERATION
// ═══════════════════════════════════════════════════════════════════════════════

use std::sync::atomic::{AtomicU64, Ordering};

static EXPRESSION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn generate_expression_id() -> String {
    let id = EXPRESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("expr_{}", id)
}

// ═══════════════════════════════════════════════════════════════════════════════
// SVG ATTRIBUTE CORRECTION
// ═══════════════════════════════════════════════════════════════════════════════

/// Correct SVG attribute casing - restores camelCase for SVG attributes
fn correct_svg_attribute_name(attr_name: &str, tag_name: &str) -> String {
    let lower_tag = tag_name.to_lowercase();
    let lower_attr = attr_name.to_lowercase();

    // Only apply SVG corrections for SVG elements
    if SVG_TAGS.contains(lower_tag.as_str()) {
        if let Some(&corrected) = SVG_ATTR_CASE_MAP.get(lower_attr.as_str()) {
            return corrected.to_string();
        }
    }

    attr_name.to_string()
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXPRESSION NORMALIZATION
// ═══════════════════════════════════════════════════════════════════════════════

/// Find the end of a balanced brace expression, handling strings and template literals.
/// Returns the index after the closing brace, or None if unbalanced.
fn find_balanced_brace_end(html: &str, start_index: usize) -> Option<usize> {
    let chars: Vec<char> = html.chars().collect();
    let mut depth = 0;
    let mut i = start_index;
    let mut in_string: Option<char> = None;
    let mut in_template_literal = false;
    let mut template_brace_depth = 0;

    while i < chars.len() {
        let c = chars[i];

        // Handle escape sequences
        if c == '\\' && i + 1 < chars.len() {
            i += 2;
            continue;
        }

        // Handle strings
        if in_string.is_some() {
            if Some(c) == in_string {
                in_string = None;
            }
            i += 1;
            continue;
        }

        // Handle template literals
        if in_template_literal {
            if c == '`' && template_brace_depth == 0 {
                in_template_literal = false;
            } else if c == '$' && i + 1 < chars.len() && chars[i + 1] == '{' {
                template_brace_depth += 1;
                i += 2;
                continue;
            } else if c == '}' && template_brace_depth > 0 {
                template_brace_depth -= 1;
            }
            i += 1;
            continue;
        }

        // Check for string delimiters
        if c == '"' || c == '\'' {
            in_string = Some(c);
            i += 1;
            continue;
        }

        // Check for template literal
        if c == '`' {
            in_template_literal = true;
            i += 1;
            continue;
        }

        // Track brace depth
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(i + 1);
            }
        }

        i += 1;
    }

    None
}

/// Normalize expressions before parsing.
/// Replaces both attr={expr} and {textExpr} with placeholders so html5ever can parse correctly.
fn normalize_all_expressions(html: &str) -> (String, HashMap<String, String>) {
    let mut normalized = String::new();
    let mut expressions = HashMap::new();
    let mut expr_counter = 0;
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        // Check for opening brace that starts an expression
        if c == '{' {
            // Make sure it's not an escaped brace or inside a string
            if let Some(end) = find_balanced_brace_end(html, i) {
                let mut expr_content: String = chars[i + 1..end - 1].iter().collect();

                // STRIP HTML COMMENTS: Expressions like { items.map(i => ( <!-- comment --> <div/> )) }
                // contain HTML comments which are invalid in JS context.
                lazy_static! {
                    static ref HTML_COMMENT_RE: Regex = Regex::new(r"(?s)<!--.*?-->").unwrap();
                }
                expr_content = HTML_COMMENT_RE.replace_all(&expr_content, "").to_string();

                let placeholder = format!("__ZENITH_EXPR_{}__", expr_counter);
                expressions.insert(placeholder.clone(), expr_content);
                normalized.push_str(&placeholder);
                expr_counter += 1;
                i = end;
                continue;
            }
        }

        normalized.push(c);
        i += 1;
    }

    (normalized, expressions)
}

/// Convert self-closing component tags to properly closed tags.
/// HTML5/html5ever treats `<ComponentName />` as an opening tag,
/// causing following siblings to be incorrectly nested as children.
fn convert_self_closing_components(html: &str) -> String {
    lazy_static! {
        static ref SELF_CLOSING_RE: Regex =
            Regex::new(r"<([A-Z][a-zA-Z0-9]*)\s*([^>]*?)\s*/>").unwrap();
    }

    SELF_CLOSING_RE
        .replace_all(html, "<$1 $2></$1>")
        .to_string()
}

/// Strip script and style blocks from HTML before parsing.
/// Preserves external script tags (<script src="...">) but removes inline scripts.
fn strip_blocks(html: &str) -> String {
    lazy_static! {
        static ref SCRIPT_RE: Regex =
            Regex::new(r"(?is)<script\b([^>]*)>([\s\S]*?)</script>").unwrap();
        static ref STYLE_RE: Regex = Regex::new(r"(?is)<style[^>]*>[\s\S]*?</style>").unwrap();
    }

    // Remove inline scripts (keep external)
    let result = SCRIPT_RE.replace_all(html, |caps: &regex::Captures| {
        let attrs = &caps[1];
        if attrs.contains("src=") {
            caps[0].to_string() // Keep external scripts
        } else {
            String::new() // Remove inline scripts
        }
    });

    // Remove styles
    STYLE_RE.replace_all(&result, "").to_string()
}

// ═══════════════════════════════════════════════════════════════════════════════
// NODE PARSING
// ═══════════════════════════════════════════════════════════════════════════════

/// Check if a tag name represents a component (starts with uppercase)
/// Pre-pass to mark component tags (uppercase) with a data attribute to preserve casing
/// because html5ever lowercases all tag names.
fn mark_component_tags(html: &str) -> String {
    lazy_static! {
        static ref TAG_OPEN_RE: Regex = Regex::new(r"<([A-Z][a-zA-Z0-9.]+)(\s|>)").unwrap();
        // Closing tags: </HeroSection>
        static ref TAG_CLOSE_RE: Regex = Regex::new(r"</([A-Z][a-zA-Z0-9.]*)>").unwrap();
    }

    let marked = TAG_OPEN_RE.replace_all(html, |caps: &regex::Captures| {
        let name = &caps[1];
        let sep = &caps[2];
        format!("<{} data-zen-orig-name=\"{}\"{}", name, name, sep)
    });

    TAG_CLOSE_RE.replace_all(&marked, "</$1>").to_string()
}

/// Check if a tag name represents a component (starts with uppercase)
pub fn is_component_tag(tag_name: &str) -> bool {
    tag_name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
}

/// Parse DOM node to TemplateNode
fn parse_dom_node(
    handle: &Handle,
    expressions: &mut Vec<ExpressionIR>,
    normalized_exprs: &HashMap<String, String>,
    parent_loop_context: Option<&LoopContext>,
    _file_path: &str,
) -> Vec<TemplateNode> {
    let node = handle;

    match &node.data {
        NodeData::Document => {
            // Process children of document
            let children = node.children.borrow();
            let mut nodes = Vec::new();
            for child in children.iter() {
                nodes.extend(parse_dom_node(
                    child,
                    expressions,
                    normalized_exprs,
                    parent_loop_context,
                    _file_path,
                ));
            }
            nodes
        }

        NodeData::Doctype {
            name,
            public_id,
            system_id,
        } => vec![TemplateNode::Doctype(DoctypeNode {
            name: name.to_string(),
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
            location: SourceLocation { line: 1, column: 1 },
        })],

        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();

            // Process text that may contain multiple expressions.
            // process_text_with_expressions handles plain text, single expressions, and mixed content.
            process_text_with_expressions(&text, expressions, normalized_exprs, parent_loop_context)
        }

        NodeData::Element { name, attrs, .. } => {
            let mut tag_name = name.local.to_string();
            let attributes = attrs.borrow();

            // CASING RESTORATION: Check if we marked this tag's original casing
            for attr in attributes.iter() {
                if attr.name.local.to_string() == "data-zen-orig-name" {
                    tag_name = attr.value.to_string();
                    break;
                }
            }

            // Parse attributes
            let mut parsed_attrs = Vec::new();
            for attr in attributes.iter() {
                let attr_name = correct_svg_attribute_name(&attr.name.local.to_string(), &tag_name);
                let attr_value = attr.value.to_string();

                // Check if attribute value contains an expression
                if let Some(caps) = EXPR_PLACEHOLDER_RE.captures(&attr_value) {
                    let placeholder = caps.get(0).unwrap().as_str();
                    if let Some(expr_code) = normalized_exprs.get(placeholder) {
                        let expr_id = generate_expression_id();
                        let expr_ir = ExpressionIR {
                            id: expr_id.clone(),
                            code: expr_code.clone(),
                            location: SourceLocation { line: 1, column: 1 },
                            loop_context: parent_loop_context.cloned(),
                        };
                        expressions.push(expr_ir.clone());
                        parsed_attrs.push(AttributeIR {
                            name: attr_name,
                            value: crate::validate::AttributeValue::Dynamic(expr_ir),
                            location: SourceLocation { line: 1, column: 1 },
                            loop_context: parent_loop_context.cloned(),
                        });
                        continue;
                    }
                }

                parsed_attrs.push(AttributeIR {
                    name: attr_name,
                    value: crate::validate::AttributeValue::Static(attr_value),
                    location: SourceLocation { line: 1, column: 1 },
                    loop_context: parent_loop_context.cloned(),
                });
            }

            // Parse children
            let children_handles = node.children.borrow();
            let mut children = Vec::new();
            for child in children_handles.iter() {
                children.extend(parse_dom_node(
                    child,
                    expressions,
                    normalized_exprs,
                    parent_loop_context,
                    _file_path,
                ));
            }

            // Check if this is a component (uppercase first letter)
            if is_component_tag(&tag_name) {
                vec![TemplateNode::Component(ComponentNode {
                    name: tag_name,
                    attributes: parsed_attrs,
                    children,
                    location: SourceLocation { line: 1, column: 1 },
                    loop_context: parent_loop_context.cloned(),
                })]
            } else {
                vec![TemplateNode::Element(ElementNode {
                    tag: tag_name,
                    attributes: parsed_attrs,
                    children,
                    location: SourceLocation { line: 1, column: 1 },
                    loop_context: parent_loop_context.cloned(),
                })]
            }
        }

        NodeData::Comment { .. } => vec![],
        NodeData::ProcessingInstruction { .. } => vec![],
    }
}

/// Process text that may contain multiple expression placeholders
fn process_text_with_expressions(
    text: &str,
    expressions: &mut Vec<ExpressionIR>,
    normalized_exprs: &HashMap<String, String>,
    loop_context: Option<&LoopContext>,
) -> Vec<TemplateNode> {
    let mut nodes = Vec::new();
    let mut last_end = 0;

    for caps in EXPR_PLACEHOLDER_RE.captures_iter(text) {
        let m = caps.get(0).unwrap();

        // Add text before this expression
        if m.start() > last_end {
            let before_text = &text[last_end..m.start()];
            if !before_text.trim().is_empty() {
                nodes.push(TemplateNode::Text(TextNode {
                    value: before_text.to_string(),
                    location: SourceLocation { line: 1, column: 1 },
                    loop_context: loop_context.cloned(),
                }));
            }
        }

        // Add expression node
        let placeholder = m.as_str();
        if let Some(expr_code) = normalized_exprs.get(placeholder) {
            let expr_id = generate_expression_id();
            expressions.push(ExpressionIR {
                id: expr_id.clone(),
                code: expr_code.clone(),
                location: SourceLocation { line: 1, column: 1 },
                loop_context: loop_context.cloned(),
            });
            nodes.push(TemplateNode::Expression(ExpressionNode {
                expression: expr_id,
                location: SourceLocation { line: 1, column: 1 },
                loop_context: loop_context.cloned(),
            }));
        }

        last_end = m.end();
    }

    // Add remaining text
    if last_end < text.len() {
        let after_text = &text[last_end..];
        if !after_text.trim().is_empty() {
            nodes.push(TemplateNode::Text(TextNode {
                value: after_text.to_string(),
                location: SourceLocation { line: 1, column: 1 },
                loop_context: loop_context.cloned(),
            }));
        }
    }

    nodes
}

// ═══════════════════════════════════════════════════════════════════════════════
// MAIN PARSING FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Parse template from HTML string
pub fn parse_template(html: &str, file_path: &str) -> Result<TemplateIR, CompilerError> {
    // Step 1: Convert self-closing components
    let html_self = convert_self_closing_components(html);

    // Step 2: Strip script and style blocks
    let html_strip = strip_blocks(&html_self);

    // Step 3: Preserve component casing (html5ever lowercases all tag names)
    let casing_preserved = mark_component_tags(&html_strip);

    // Step 4: Normalize expressions to placeholders
    let (normalized, normalized_exprs) = normalize_all_expressions(&casing_preserved);

    // Step 5: Parse with html5ever
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut normalized.as_bytes())
        .map_err(|e| {
            CompilerError::new(
                "PARSE_ERROR",
                &format!("Failed to parse HTML: {}", e),
                file_path,
                0,
                0,
            )
        })?;

    // Step 5: Convert DOM to TemplateNodes
    let mut expressions = Vec::new();
    let mut nodes = Vec::new();

    // Check if original source had document tags
    let has_html_in_src = html.to_lowercase().contains("<html");

    fn collect_body_content(
        handle: &Handle,
        nodes: &mut Vec<TemplateNode>,
        expressions: &mut Vec<ExpressionIR>,
        normalized_exprs: &HashMap<String, String>,
        file_path: &str,
        has_html_in_src: bool,
    ) {
        let node = handle;
        match &node.data {
            NodeData::Document => {
                for child in node.children.borrow().iter() {
                    collect_body_content(
                        child,
                        nodes,
                        expressions,
                        normalized_exprs,
                        file_path,
                        has_html_in_src,
                    );
                }
            }
            NodeData::Element { name, .. } => {
                let tag = name.local.to_string().to_lowercase();

                // If it's a wrapper tag (html, head, body) and NOT in the original source,
                // we flatten it (recurse into children without adding the tag).
                // This prevents "9 Heads and 9 Bodies" issue while preserving layouts.
                let is_wrapper = tag == "html" || tag == "head" || tag == "body";
                if is_wrapper && !has_html_in_src {
                    for child in node.children.borrow().iter() {
                        collect_body_content(
                            child,
                            nodes,
                            expressions,
                            normalized_exprs,
                            file_path,
                            has_html_in_src,
                        );
                    }
                } else if tag == "html" {
                    // Always recurse into <html> but keep it as a node if it was in src?
                    // Actually, if has_html_in_src is true, we want to keep the tree structure
                    // but html5ever already ensures <html> is the parent.
                    // To avoid double wrappers, we recurse into html but keep head and body.
                    for child in node.children.borrow().iter() {
                        collect_body_content(
                            child,
                            nodes,
                            expressions,
                            normalized_exprs,
                            file_path,
                            has_html_in_src,
                        );
                    }
                } else {
                    nodes.extend(parse_dom_node(
                        handle,
                        expressions,
                        normalized_exprs,
                        None,
                        file_path,
                    ));
                }
            }
            NodeData::Doctype { .. } => {
                if has_html_in_src {
                    nodes.extend(parse_dom_node(
                        handle,
                        expressions,
                        normalized_exprs,
                        None,
                        file_path,
                    ));
                }
            }
            _ => {}
        }
    }

    collect_body_content(
        &dom.document,
        &mut nodes,
        &mut expressions,
        &normalized_exprs,
        file_path,
        has_html_in_src,
    );

    Ok(TemplateIR {
        raw: html.to_string(),
        nodes,
        expressions,
    })
}

/// Parse script block from HTML string
pub fn parse_script(html: &str) -> Option<ScriptIR> {
    let mut scripts = Vec::new();
    let mut attributes = HashMap::new();

    for caps in SCRIPT_REGEX.captures_iter(html) {
        let attr_string = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let content = caps.get(2).map(|m| m.as_str()).unwrap_or("");

        // Parse attributes
        for attr_caps in ATTR_REGEX.captures_iter(attr_string) {
            if let Some(name) = attr_caps.get(1) {
                let value = attr_caps
                    .get(2)
                    .or_else(|| attr_caps.get(3))
                    .or_else(|| attr_caps.get(4))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_else(|| "true".to_string());
                attributes.insert(name.as_str().to_string(), value);
            }
        }

        if !content.trim().is_empty() {
            scripts.push(content.trim().to_string());
        }
    }

    if scripts.is_empty() {
        return None;
    }

    Some(ScriptIR {
        raw: scripts.join("\n\n"),
        attributes,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// NAPI EXPORTS
// ═══════════════════════════════════════════════════════════════════════════════

#[napi]
pub fn parse_template_native(html: String, file_path: String) -> napi::Result<serde_json::Value> {
    let ir = parse_template(&html, &file_path).map_err(|e| napi::Error::from_reason(e.message))?;
    serde_json::to_value(ir).map_err(|e| napi::Error::from_reason(e.to_string()))
}

#[napi]
pub fn parse_script_native(html: String) -> Option<serde_json::Value> {
    parse_script(&html).and_then(|ir| serde_json::to_value(ir).ok())
}

#[napi]
pub fn is_component_tag_native(tag_name: String) -> bool {
    is_component_tag(&tag_name)
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_component_tag() {
        assert!(is_component_tag("Button"));
        assert!(is_component_tag("HeroSection"));
        assert!(!is_component_tag("div"));
        assert!(!is_component_tag("span"));
    }

    #[test]
    fn test_svg_attribute_correction() {
        assert_eq!(correct_svg_attribute_name("viewbox", "svg"), "viewBox");
        assert_eq!(
            correct_svg_attribute_name("preserveaspectratio", "svg"),
            "preserveAspectRatio"
        );
        assert_eq!(correct_svg_attribute_name("class", "svg"), "class"); // Not in map
        assert_eq!(correct_svg_attribute_name("viewbox", "div"), "viewbox"); // Not SVG element
    }

    #[test]
    fn test_find_balanced_brace() {
        assert_eq!(find_balanced_brace_end("{hello}", 0), Some(7));
        assert_eq!(find_balanced_brace_end("{a + b}", 0), Some(7));
        assert_eq!(find_balanced_brace_end("{obj.map(x => x)}", 0), Some(17));
        assert_eq!(
            find_balanced_brace_end("{'string with { brace'}", 0),
            Some(23)
        );
    }

    #[test]
    fn test_normalize_expressions() {
        let (normalized, exprs) = normalize_all_expressions("<div>{count}</div>");
        assert!(normalized.contains("__ZENITH_EXPR_"));
        assert_eq!(exprs.len(), 1);
        assert!(exprs.values().any(|v| v == "count"));
    }

    #[test]
    fn test_convert_self_closing() {
        let result = convert_self_closing_components("<Button />");
        assert_eq!(result, "<Button ></Button>");

        let result = convert_self_closing_components("<Card prop=\"value\" />");
        assert!(result.contains("<Card"));
        assert!(result.contains("</Card>"));
    }

    #[test]
    fn test_parse_script() {
        let html = r#"<script setup lang="ts">const x = 1;</script>"#;
        let script = parse_script(html);
        assert!(script.is_some());
        let script = script.unwrap();
        assert!(script.raw.contains("const x = 1"));
        assert_eq!(script.attributes.get("setup"), Some(&"true".to_string()));
        assert_eq!(script.attributes.get("lang"), Some(&"ts".to_string()));
    }
}
