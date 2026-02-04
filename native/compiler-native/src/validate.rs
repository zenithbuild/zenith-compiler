#[cfg(feature = "napi")]
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ═══════════════════════════════════════════════════════════════════════════════
// INVARIANT CODES
// ═══════════════════════════════════════════════════════════════════════════════

pub const INV_LOOP_CONTEXT_LOST: &str = "INV001";
pub const INV_ATTRIBUTE_NOT_FORWARDED: &str = "INV002";
pub const INV_UNRESOLVED_COMPONENT: &str = "INV003";
pub const INV_REACTIVE_BOUNDARY: &str = "INV004";
pub const INV_TEMPLATE_TAG: &str = "INV005";
pub const INV_SLOT_ATTRIBUTE: &str = "INV006";
pub const INV_ORPHAN_COMPOUND: &str = "INV007";
pub const INV_NON_ENUMERABLE_JSX: &str = "INV008";
pub const INV_UNREGISTERED_EXPRESSION: &str = "INV009";
pub const INV_COMPONENT_PRECOMPILED: &str = "INV010";
pub const INV_UNRESOLVED_IDENTIFIER: &str = "Z-ERR-SCOPE-002";
pub const INV_LAYOUT_FORBIDDEN: &str = "Z-ERR-LAYOUT-FORBIDDEN";
pub const INV_RUN_REACTIVE: &str = "Z-ERR-RUN-REACTIVE";
pub const INV_REACTIVITY_BOUNDARY: &str = "Z-ERR-REACTIVITY-BOUNDARY";

// ═══════════════════════════════════════════════════════════════════════════════
// SCOPE BINDINGS (Phase 1: Identifier Inventory)
// ═══════════════════════════════════════════════════════════════════════════════

/// Represents the complete set of valid identifiers for a component instance.
/// This is the source of truth for identifier classification.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScopeBindings {
    /// State variable names (reactive, declared with `state`)
    pub state_names: HashSet<String>,
    /// Prop names (passed from parent, declared with `prop`)
    pub prop_names: HashSet<String>,
    /// Local variable names (const/let/function declarations)
    pub local_names: HashSet<String>,
}

impl ScopeBindings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from existing HashSets
    pub fn from_sets(
        state_names: HashSet<String>,
        prop_names: HashSet<String>,
        local_names: HashSet<String>,
    ) -> Self {
        Self {
            state_names,
            prop_names,
            local_names,
        }
    }

    /// Returns the category for an identifier, or None if unresolved.
    /// Classification priority: locals > state > props
    pub fn classify(&self, name: &str) -> Option<IdentifierCategory> {
        if self.local_names.contains(name) {
            Some(IdentifierCategory::Local)
        } else if self.state_names.contains(name) {
            Some(IdentifierCategory::State)
        } else if self.prop_names.contains(name) {
            Some(IdentifierCategory::Prop)
        } else {
            None
        }
    }

    /// Check if any bindings exist
    pub fn is_empty(&self) -> bool {
        self.state_names.is_empty() && self.prop_names.is_empty() && self.local_names.is_empty()
    }
}

/// Classification of an identifier's binding category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierCategory {
    State,
    Prop,
    Local,
}

// ═══════════════════════════════════════════════════════════════════════════════
// GUARANTEES
// ═══════════════════════════════════════════════════════════════════════════════

fn get_guarantee(code: &str) -> &'static str {
    match code {
        INV_LOOP_CONTEXT_LOST => "Slot content retains its original reactive scope.",
        INV_ATTRIBUTE_NOT_FORWARDED => {
            "Attributes passed to components are forwarded to the semantic root element."
        }
        INV_UNRESOLVED_COMPONENT => "All components are resolved at compile time.",
        INV_REACTIVE_BOUNDARY => "Components are purely structural transforms.",
        INV_TEMPLATE_TAG => "Named slots use compound component pattern, not <template> tags.",
        INV_SLOT_ATTRIBUTE => {
            "Named slots use compound component pattern, not slot=\"\" attributes."
        }
        INV_ORPHAN_COMPOUND => {
            "Compound slot markers must be direct children of their parent component."
        }
        INV_NON_ENUMERABLE_JSX => "JSX expressions must have statically enumerable output.",
        INV_UNREGISTERED_EXPRESSION => {
            "All bindings must reference an ID that exists in the registry."
        }
        INV_COMPONENT_PRECOMPILED => "Component AST must be precompiled before instantiation.",
        INV_LAYOUT_FORBIDDEN => "Layouts are deprecated. Use component wrapping instead.",
        INV_RUN_REACTIVE => "Component __run() must not reference reactive state or props. Use effects or expressions for reactive behavior.",
        INV_REACTIVITY_BOUNDARY => "Reactive state may only be read inside expressions. Reactive state may only be written inside event handlers.",
        _ => "Unknown invariant.",
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMPILER ERROR
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "napi", napi(object))]
pub struct CompilerError {
    pub code: String,
    pub error_type: String,
    pub message: String,
    pub guarantee: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub context: Option<String>,
    pub hints: Vec<String>,
}

impl CompilerError {
    pub fn new(code: &str, message: &str, file: &str, line: u32, column: u32) -> Self {
        Self::with_details(code, message, file, line, column, None, vec![])
    }

    pub fn with_details(
        code: &str,
        message: &str,
        file: &str,
        line: u32,
        column: u32,
        context: Option<String>,
        hints: Vec<String>,
    ) -> Self {
        CompilerError {
            code: code.to_string(),
            error_type: "COMPILER_INVARIANT_VIOLATION".to_string(), // Default type
            message: message.to_string(),
            guarantee: get_guarantee(code).to_string(),
            file: file.to_string(),
            line,
            column,
            context,
            hints,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// IR TYPES
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "napi", napi(object))]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "napi", napi(object))]
#[serde(rename_all = "camelCase")]
pub struct LoopContext {
    pub variables: Vec<String>,
    pub map_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopContextInput {
    pub variables: Vec<String>,
    #[serde(default)]
    pub map_source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionInput {
    pub id: String,
    pub code: String,
    pub loop_context: Option<LoopContextInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionIR {
    #[serde(default)]
    pub id: String,
    pub code: String,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TemplateNode {
    Element(ElementNode),
    Text(TextNode),
    Expression(ExpressionNode),
    Component(ComponentNode),
    ConditionalFragment(ConditionalFragmentNode),
    OptionalFragment(OptionalFragmentNode),
    LoopFragment(LoopFragmentNode),
    Doctype(DoctypeNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementNode {
    pub tag: String,
    pub attributes: Vec<AttributeIR>,
    pub children: Vec<TemplateNode>,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextNode {
    pub value: String,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionNode {
    pub expression: String,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
    /// If true, this expression is inside <head> and must be statically resolvable
    #[serde(default)]
    pub is_in_head: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentNode {
    pub name: String,
    pub attributes: Vec<AttributeIR>,
    pub children: Vec<TemplateNode>,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionalFragmentNode {
    pub condition: String,
    pub consequent: Vec<TemplateNode>,
    pub alternate: Vec<TemplateNode>,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalFragmentNode {
    pub condition: String,
    pub fragment: Vec<TemplateNode>,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopFragmentNode {
    pub source: String,
    pub item_var: String,
    pub index_var: Option<String>,
    pub body: Vec<TemplateNode>,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctypeNode {
    pub name: String,
    pub public_id: String,
    pub system_id: String,
    #[serde(default)]
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    Static(String),
    Dynamic(ExpressionIR),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributeIR {
    pub name: String,
    pub value: AttributeValue,
    #[serde(default)]
    pub location: SourceLocation,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateIR {
    pub raw: String,
    pub nodes: Vec<TemplateNode>,
    pub expressions: Vec<ExpressionIR>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptIR {
    pub raw: String,
    pub attributes: HashMap<String, String>,
    #[serde(default)]
    pub states: HashMap<String, String>,
    #[serde(default)]
    pub props: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleIR {
    pub raw: String,
}

/// Meta tag for head directive
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MetaTag {
    pub name: Option<String>,
    pub property: Option<String>,
    pub content: String,
}

/// Link tag for head directive
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LinkTag {
    pub rel: String,
    pub href: String,
    #[serde(default)]
    pub r#type: Option<String>,
}

/// Head directive for compile-time head element injection
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct HeadDirective {
    pub title: Option<String>,
    pub description: Option<String>,
    pub meta: Vec<MetaTag>,
    pub links: Vec<LinkTag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZenIR {
    pub file_path: String,
    pub template: TemplateIR,
    pub script: Option<ScriptIR>,
    pub styles: Vec<StyleIR>,
    #[serde(default)]
    pub props: Vec<String>,
    #[serde(default)]
    pub page_bindings: Vec<String>,
    #[serde(default)]
    pub page_props: Vec<String>,
    #[serde(default)]
    pub all_states: HashMap<String, String>,
    /// Head directive for compile-time <head> element injection
    #[serde(default)]
    pub head_directive: Option<HeadDirective>,
    /// Whether this component/page uses reactive state
    #[serde(default)]
    pub uses_state: bool,
    /// Whether this component/page has event handlers
    #[serde(default)]
    pub has_events: bool,
    /// CSS class names used (for pruning)
    #[serde(default)]
    pub css_classes: Vec<String>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// VALIDATION FUNCTIONS (Return Option, not Result)
// ═══════════════════════════════════════════════════════════════════════════════

fn validate_no_unresolved_components(nodes: &[TemplateNode], file: &str) -> Option<CompilerError> {
    for node in nodes {
        if let Some(e) = check_node_for_unresolved_component(node, file) {
            return Some(e);
        }
    }
    None
}

fn check_node_for_unresolved_component(node: &TemplateNode, file: &str) -> Option<CompilerError> {
    match node {
        TemplateNode::Component(c) => Some(CompilerError::new(
            INV_UNRESOLVED_COMPONENT,
            &format!("Unresolved component: <{}>.", c.name),
            file,
            c.location.line,
            c.location.column,
        )),
        TemplateNode::Element(e) => {
            for child in &e.children {
                if let Some(err) = check_node_for_unresolved_component(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::ConditionalFragment(cf) => {
            for child in &cf.consequent {
                if let Some(err) = check_node_for_unresolved_component(child, file) {
                    return Some(err);
                }
            }
            for child in &cf.alternate {
                if let Some(err) = check_node_for_unresolved_component(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::OptionalFragment(of) => {
            for child in &of.fragment {
                if let Some(err) = check_node_for_unresolved_component(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::LoopFragment(lf) => {
            for child in &lf.body {
                if let Some(err) = check_node_for_unresolved_component(child, file) {
                    return Some(err);
                }
            }
            None
        }
        _ => None,
    }
}

/// Phase A6: Validate that no Layout components are used (layouts are now just components)
fn validate_no_layouts(nodes: &[TemplateNode], file: &str) -> Option<CompilerError> {
    for node in nodes {
        if let Some(e) = check_node_for_layout(node, file) {
            return Some(e);
        }
    }
    None
}

fn check_node_for_layout(node: &TemplateNode, file: &str) -> Option<CompilerError> {
    match node {
        TemplateNode::Component(c) => {
            // Detect Layout components by name pattern
            if c.name.to_lowercase().contains("layout") {
                return Some(CompilerError::with_details(
                    INV_LAYOUT_FORBIDDEN,
                    &format!("<{}> detected. Layouts are deprecated.", c.name),
                    file,
                    c.location.line,
                    c.location.column,
                    Some(format!("<{}>", c.name)),
                    vec![
                        "Use component wrapping instead of layouts.".to_string(),
                        "Layouts are now just: <Component>children</Component>".to_string(),
                    ],
                ));
            }
            // Recurse into children
            for child in &c.children {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::Element(e) => {
            for child in &e.children {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::ConditionalFragment(cf) => {
            for child in &cf.consequent {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            for child in &cf.alternate {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::OptionalFragment(of) => {
            for child in &of.fragment {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::LoopFragment(lf) => {
            for child in &lf.body {
                if let Some(err) = check_node_for_layout(child, file) {
                    return Some(err);
                }
            }
            None
        }
        _ => None,
    }
}

fn validate_no_template_tags(nodes: &[TemplateNode], file: &str) -> Option<CompilerError> {
    for node in nodes {
        if let Some(e) = check_node_for_template_tag(node, file) {
            return Some(e);
        }
    }
    None
}

fn check_node_for_template_tag(node: &TemplateNode, file: &str) -> Option<CompilerError> {
    match node {
        TemplateNode::Element(e) => {
            if e.tag == "template" {
                return Some(CompilerError::with_details(
                    INV_TEMPLATE_TAG,
                    "<template> tags are forbidden.",
                    file,
                    e.location.line,
                    e.location.column,
                    Some("<template>".to_string()),
                    vec![
                        "Use a Zenith component or a standard HTML element instead.".to_string(),
                        "Named slots should use the compound component pattern.".to_string(),
                    ],
                ));
            }
            for child in &e.children {
                if let Some(err) = check_node_for_template_tag(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::ConditionalFragment(cf) => {
            for child in &cf.consequent {
                if let Some(err) = check_node_for_template_tag(child, file) {
                    return Some(err);
                }
            }
            for child in &cf.alternate {
                if let Some(err) = check_node_for_template_tag(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::OptionalFragment(of) => {
            for child in &of.fragment {
                if let Some(err) = check_node_for_template_tag(child, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::LoopFragment(lf) => {
            for child in &lf.body {
                if let Some(err) = check_node_for_template_tag(child, file) {
                    return Some(err);
                }
            }
            None
        }
        _ => None,
    }
}

fn validate_expressions_registered(
    nodes: &[TemplateNode],
    expressions: &[ExpressionIR],
    file: &str,
) -> Option<CompilerError> {
    let registry: HashSet<&str> = expressions.iter().map(|e| e.id.as_str()).collect();
    for node in nodes {
        if let Some(e) = check_node_expressions(node, &registry, file) {
            return Some(e);
        }
    }
    None
}

fn check_node_expressions(
    node: &TemplateNode,
    registry: &HashSet<&str>,
    file: &str,
) -> Option<CompilerError> {
    match node {
        TemplateNode::Expression(e) => {
            if !registry.contains(e.expression.as_str()) {
                return Some(CompilerError::new(
                    INV_UNREGISTERED_EXPRESSION,
                    &format!("Expression ID \"{}\" missing from registry.", e.expression),
                    file,
                    e.location.line,
                    e.location.column,
                ));
            }
            None
        }
        TemplateNode::Element(el) => {
            for attr in &el.attributes {
                if let AttributeValue::Dynamic(expr) = &attr.value {
                    if !registry.contains(expr.id.as_str()) {
                        return Some(CompilerError::new(
                            INV_UNREGISTERED_EXPRESSION,
                            &format!(
                                "Attr \"{}\" references missing ID \"{}\".",
                                attr.name, expr.id
                            ),
                            file,
                            attr.location.line,
                            attr.location.column,
                        ));
                    }
                }
            }
            for child in &el.children {
                if let Some(err) = check_node_expressions(child, registry, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::ConditionalFragment(cf) => {
            if !registry.contains(cf.condition.as_str()) {
                return Some(CompilerError::new(
                    INV_UNREGISTERED_EXPRESSION,
                    &format!("Condition ID \"{}\" missing.", cf.condition),
                    file,
                    cf.location.line,
                    cf.location.column,
                ));
            }
            for child in &cf.consequent {
                if let Some(err) = check_node_expressions(child, registry, file) {
                    return Some(err);
                }
            }
            for child in &cf.alternate {
                if let Some(err) = check_node_expressions(child, registry, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::OptionalFragment(of) => {
            if !registry.contains(of.condition.as_str()) {
                return Some(CompilerError::new(
                    INV_UNREGISTERED_EXPRESSION,
                    &format!("Optional condition ID \"{}\" missing.", of.condition),
                    file,
                    of.location.line,
                    of.location.column,
                ));
            }
            for child in &of.fragment {
                if let Some(err) = check_node_expressions(child, registry, file) {
                    return Some(err);
                }
            }
            None
        }
        TemplateNode::LoopFragment(lf) => {
            if !registry.contains(lf.source.as_str()) {
                return Some(CompilerError::new(
                    INV_UNREGISTERED_EXPRESSION,
                    &format!("Loop source ID \"{}\" missing.", lf.source),
                    file,
                    lf.location.line,
                    lf.location.column,
                ));
            }
            for child in &lf.body {
                if let Some(err) = check_node_expressions(child, registry, file) {
                    return Some(err);
                }
            }
            None
        }
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// NAPI ENTRY POINT
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "napi")]
#[napi]
pub fn validate_ir(ir_json: String) -> Option<CompilerError> {
    let ir: ZenIR = match serde_json::from_str(&ir_json) {
        Ok(parsed) => parsed,
        Err(e) => {
            return Some(CompilerError::new(
                "PARSE_ERROR",
                &format!("Failed to parse IR JSON: {}", e),
                "unknown",
                1,
                1,
            ));
        }
    };

    let file = &ir.file_path;

    if let Some(e) = validate_no_unresolved_components(&ir.template.nodes, file) {
        return Some(e);
    }

    // Phase A6: Reject any Layout component usage
    if let Some(e) = validate_no_layouts(&ir.template.nodes, file) {
        return Some(e);
    }

    if let Some(e) = validate_no_template_tags(&ir.template.nodes, file) {
        return Some(e);
    }

    if let Some(e) =
        validate_expressions_registered(&ir.template.nodes, &ir.template.expressions, file)
    {
        return Some(e);
    }

    None
}
