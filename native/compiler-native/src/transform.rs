use lazy_static::lazy_static;
use napi_derive::napi;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::validate::{
    AttributeIR, AttributeValue, ComponentNode, ConditionalFragmentNode, ElementNode, ExpressionIR,
    ExpressionInput, ExpressionNode, LoopContext, LoopContextInput, LoopFragmentNode,
    OptionalFragmentNode, SourceLocation, TemplateNode, TextNode,
};

// ═══════════════════════════════════════════════════════════════════════════════
// EXPRESSION CLASSIFICATION TYPES
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExpressionOutputType {
    Primitive,
    Conditional,
    Optional,
    Loop,
    Fragment,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpressionClassification {
    #[serde(rename = "type")]
    pub expr_type: ExpressionOutputType,
    pub condition: Option<String>,
    pub consequent: Option<String>,
    pub alternate: Option<String>,
    pub optional_condition: Option<String>,
    pub optional_fragment: Option<String>,
    pub loop_source: Option<String>,
    pub loop_item_var: Option<String>,
    pub loop_index_var: Option<String>,
    pub loop_body: Option<String>,
    pub fragment_code: Option<String>,
}

impl Default for ExpressionClassification {
    fn default() -> Self {
        ExpressionClassification {
            expr_type: ExpressionOutputType::Primitive,
            condition: None,
            consequent: None,
            alternate: None,
            optional_condition: None,
            optional_fragment: None,
            loop_source: None,
            loop_item_var: None,
            loop_index_var: None,
            loop_body: None,
            fragment_code: None,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LOWERING CONTEXT
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct LoweringContext<'a> {
    pub expressions: &'a mut Vec<ExpressionIR>,
    pub file_path: String,
}

// ═══════════════════════════════════════════════════════════════════════════════
// BINDING TABLE
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default)]
pub struct BindingTable {
    bindings: HashSet<String>,
    frozen: bool,
}

impl BindingTable {
    pub fn new() -> Self {
        BindingTable::default()
    }

    pub fn add(&mut self, name: &str) -> Result<(), String> {
        if self.frozen {
            return Err(format!(
                "Cannot add binding \"{}\" after table is frozen.",
                name
            ));
        }
        self.bindings.insert(name.to_string());
        Ok(())
    }

    pub fn has(&self, name: &str) -> bool {
        self.bindings.contains(name)
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn bindings(&self) -> &HashSet<String> {
        &self.bindings
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXPRESSION ANALYSIS INPUT/OUTPUT
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisInput {
    pub expressions: Vec<ExpressionInput>,
    pub known_bindings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
pub struct ExpressionAnalysisResult {
    pub id: String,
    pub classification: String, // JSON-serialized ExpressionClassification
    pub dependencies: Vec<String>,
    pub uses_state: bool,
    pub uses_loop_context: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
pub struct EvaluatedExpression {
    pub id: String,
    pub expression_type: String,
    pub dependencies: Vec<String>,
    pub uses_state: bool,
    pub uses_loop_context: bool,
    pub classification_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
pub struct AnalysisOutput {
    pub results: Vec<ExpressionAnalysisResult>,
    pub binding_count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════════
// CLASSIFICATION HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

fn contains_jsx(code: &str) -> bool {
    lazy_static! {
        static ref JSX_RE: Regex = Regex::new(r"<[a-zA-Z]").unwrap();
    }
    JSX_RE.is_match(code)
}

fn parse_map_expression(code: &str) -> Option<(String, String, Option<String>, String)> {
    let map_index = code.find(".map(")?;
    let source = code[..map_index].trim().to_string();
    if source.is_empty() {
        return None;
    }

    let after_map = &code[map_index + 5..].trim_start();

    let (item_var, index_var, body_start_offset) = if after_map.starts_with('(') {
        let close_paren = find_balanced_paren(after_map, 0)?;
        let params_str = &after_map[1..close_paren];
        let params: Vec<&str> = params_str.split(',').map(|p| p.trim()).collect();
        let item = params.get(0).copied().unwrap_or("").to_string();
        let index = params.get(1).map(|s| s.to_string());
        let after_params = &after_map[close_paren + 1..].trim_start();
        if !after_params.starts_with("=>") {
            return None;
        }
        (
            item,
            index,
            close_paren
                + 1
                + (after_map.len() - after_map[close_paren + 1..].trim_start().len())
                + 2,
        )
    } else {
        let arrow_index = after_map.find("=>")?;
        let item = after_map[..arrow_index].trim().to_string();
        (item, None, arrow_index + 2)
    };

    if item_var.is_empty() {
        return None;
    }

    let body_text = after_map.get(body_start_offset..)?;
    let body = if body_text.ends_with(')') {
        &body_text[..body_text.len() - 1]
    } else {
        body_text
    }
    .trim();

    if !contains_jsx(body) {
        return None;
    }
    Some((source, item_var, index_var, body.to_string()))
}

fn find_balanced_paren(code: &str, start_index: usize) -> Option<usize> {
    let bytes = code.as_bytes();
    if bytes.get(start_index)? != &b'(' {
        return None;
    }
    let mut depth = 1;
    let mut i = start_index + 1;
    while i < bytes.len() && depth > 0 {
        if bytes[i] == b'(' {
            depth += 1;
        } else if bytes[i] == b')' {
            depth -= 1;
        }
        i += 1;
    }
    if depth == 0 {
        Some(i - 1)
    } else {
        None
    }
}

fn parse_ternary_expression(code: &str) -> Option<(String, String, String)> {
    let question_index = find_ternary_operator(code)?;
    let condition = code[..question_index].trim().to_string();
    let after_question = &code[question_index + 1..];
    let colon_index = find_ternary_colon(after_question)?;
    let consequent = after_question[..colon_index].trim().to_string();
    let alternate = after_question[colon_index + 1..].trim().to_string();
    if condition.is_empty() || consequent.is_empty() || alternate.is_empty() {
        return None;
    }
    Some((condition, consequent, alternate))
}

fn find_ternary_operator(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = 0u8;
    for i in 0..bytes.len() {
        let c = bytes[i];
        if i > 0 && bytes[i - 1] == b'\\' {
            continue;
        }
        if !in_string && (c == b'"' || c == b'\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string && c == string_char {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }
        if c == b'(' || c == b'{' || c == b'[' {
            depth += 1;
        }
        if c == b')' || c == b'}' || c == b']' {
            depth -= 1;
        }
        if c == b'?' && depth == 0 {
            return Some(i);
        }
    }
    None
}

fn find_ternary_colon(code: &str) -> Option<usize> {
    let bytes = code.as_bytes();
    let mut depth = 0;
    let mut ternary_depth = 0;
    let mut in_string = false;
    let mut string_char = 0u8;
    for i in 0..bytes.len() {
        let c = bytes[i];
        if i > 0 && bytes[i - 1] == b'\\' {
            continue;
        }
        if !in_string && (c == b'"' || c == b'\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string && c == string_char {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }
        if c == b'(' || c == b'{' || c == b'[' {
            depth += 1;
        }
        if c == b')' || c == b'}' || c == b']' {
            depth -= 1;
        }
        if c == b'?' {
            ternary_depth += 1;
        }
        if c == b':' && ternary_depth > 0 {
            ternary_depth -= 1;
            continue;
        }
        if c == b':' && depth == 0 && ternary_depth == 0 {
            return Some(i);
        }
    }
    None
}

fn parse_logical_and_expression(code: &str) -> Option<(String, String)> {
    let bytes = code.as_bytes();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = 0u8;
    for i in 0..bytes.len().saturating_sub(1) {
        let c = bytes[i];
        let next = bytes[i + 1];
        if i > 0 && bytes[i - 1] == b'\\' {
            continue;
        }
        if !in_string && (c == b'"' || c == b'\'') {
            in_string = true;
            string_char = c;
            continue;
        }
        if in_string && c == string_char {
            in_string = false;
            continue;
        }
        if in_string {
            continue;
        }
        if c == b'(' || c == b'{' || c == b'[' {
            depth += 1;
        }
        if c == b')' || c == b'}' || c == b']' {
            depth -= 1;
        }
        if c == b'&' && next == b'&' && depth == 0 {
            let condition = code[..i].trim().to_string();
            let fragment = code[i + 2..].trim().to_string();
            if !condition.is_empty() && !fragment.is_empty() {
                return Some((condition, fragment));
            }
        }
    }
    None
}

pub fn classify_expression(code: &str) -> ExpressionClassification {
    let trimmed = code.trim();
    if let Some((source, item_var, index_var, body)) = parse_map_expression(trimmed) {
        return ExpressionClassification {
            expr_type: ExpressionOutputType::Loop,
            loop_source: Some(source),
            loop_item_var: Some(item_var),
            loop_index_var: index_var,
            loop_body: Some(body),
            ..Default::default()
        };
    }
    if let Some((condition, consequent, alternate)) = parse_ternary_expression(trimmed) {
        if contains_jsx(&consequent) || contains_jsx(&alternate) {
            return ExpressionClassification {
                expr_type: ExpressionOutputType::Conditional,
                condition: Some(condition),
                consequent: Some(consequent),
                alternate: Some(alternate),
                ..Default::default()
            };
        }
    }
    if let Some((condition, fragment)) = parse_logical_and_expression(trimmed) {
        if contains_jsx(&fragment) {
            return ExpressionClassification {
                expr_type: ExpressionOutputType::Optional,
                optional_condition: Some(condition),
                optional_fragment: Some(fragment),
                ..Default::default()
            };
        }
    }
    ExpressionClassification::default()
}

// ═══════════════════════════════════════════════════════════════════════════════
// DEPENDENCY EXTRACTION
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_identifiers(code: &str) -> HashSet<String> {
    lazy_static! {
        static ref IDENT_RE: Regex = Regex::new(r"\b([a-zA-Z_$][a-zA-Z0-9_$]*)\b").unwrap();
    }
    IDENT_RE
        .captures_iter(code)
        .filter_map(|cap: regex::Captures| cap.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

fn compute_dependencies(
    code: &str,
    known_bindings: &HashSet<String>,
    loop_context: &Option<LoopContextInput>,
) -> (Vec<String>, bool, bool) {
    let identifiers = extract_identifiers(code);
    let mut dependencies = Vec::new();
    let mut uses_state = false;
    let mut uses_loop_context = false;
    let loop_vars: HashSet<String> = loop_context
        .as_ref()
        .map(|lc| lc.variables.iter().cloned().collect())
        .unwrap_or_default();
    for ident in identifiers {
        if loop_vars.contains(&ident) {
            uses_loop_context = true;
        } else if known_bindings.contains(&ident) {
            uses_state = true;
            dependencies.push(ident);
        }
    }
    dependencies.sort();
    dependencies.dedup();
    (dependencies, uses_state, uses_loop_context)
}

// ═══════════════════════════════════════════════════════════════════════════════
// FRAGMENT LOWERING
// ═══════════════════════════════════════════════════════════════════════════════

#[napi]
pub fn lower_fragments_native(
    nodes_json: String,
    expressions_json: String,
    file_path: String,
) -> napi::Result<String> {
    let mut nodes: Vec<TemplateNode> = serde_json::from_str(&nodes_json)
        .map_err(|e| napi::Error::from_reason(format!("Nodes parse error: {}", e)))?;
    let mut expressions: Vec<ExpressionIR> = serde_json::from_str(&expressions_json)
        .map_err(|e| napi::Error::from_reason(format!("Expressions parse error: {}", e)))?;
    {
        let mut ctx = LoweringContext {
            expressions: &mut expressions,
            file_path,
        };
        nodes = lower_fragments(nodes, &mut ctx);
    }
    let res = serde_json::json!({ "nodes": nodes, "expressions": expressions });
    serde_json::to_string(&res)
        .map_err(|e| napi::Error::from_reason(format!("Serialize error: {}", e)))
}

pub fn lower_fragments(nodes: Vec<TemplateNode>, ctx: &mut LoweringContext) -> Vec<TemplateNode> {
    nodes
        .into_iter()
        .map(|node| lower_node(node, ctx))
        .collect()
}

fn lower_node(node: TemplateNode, ctx: &mut LoweringContext) -> TemplateNode {
    match node {
        TemplateNode::Expression(expr_node) => lower_expression_node(expr_node, ctx),
        TemplateNode::Element(mut elem) => {
            elem.children = lower_fragments(elem.children, ctx);
            TemplateNode::Element(elem)
        }
        TemplateNode::Component(mut comp) => {
            comp.children = lower_fragments(comp.children, ctx);
            TemplateNode::Component(comp)
        }
        TemplateNode::ConditionalFragment(mut cond) => {
            cond.consequent = lower_fragments(cond.consequent, ctx);
            cond.alternate = lower_fragments(cond.alternate, ctx);
            TemplateNode::ConditionalFragment(cond)
        }
        TemplateNode::OptionalFragment(mut opt) => {
            opt.fragment = lower_fragments(opt.fragment, ctx);
            TemplateNode::OptionalFragment(opt)
        }
        TemplateNode::LoopFragment(mut lp) => {
            lp.body = lower_fragments(lp.body, ctx);
            TemplateNode::LoopFragment(lp)
        }
        _ => node,
    }
}

fn lower_expression_node(node: ExpressionNode, ctx: &mut LoweringContext) -> TemplateNode {
    let expr_id = &node.expression;
    let expr_code = ctx
        .expressions
        .iter()
        .find(|e| &e.id == expr_id)
        .map(|e| e.code.clone());
    let code = match expr_code {
        Some(c) => c,
        None => return TemplateNode::Expression(node),
    };
    let class = classify_expression(&code);
    match class.expr_type {
        ExpressionOutputType::Conditional => lower_conditional_expression(node, class, ctx),
        ExpressionOutputType::Optional => lower_optional_expression(node, class, ctx),
        ExpressionOutputType::Loop => lower_loop_expression(node, class, ctx),
        _ => TemplateNode::Expression(node),
    }
}

fn lower_conditional_expression(
    node: ExpressionNode,
    class: ExpressionClassification,
    ctx: &mut LoweringContext,
) -> TemplateNode {
    let cond_id = register_expression_typed(
        "cond",
        class.condition.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    let consequent = parse_jsx_to_nodes(
        &class.consequent.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    let alternate = parse_jsx_to_nodes(
        &class.alternate.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    TemplateNode::ConditionalFragment(ConditionalFragmentNode {
        condition: cond_id,
        consequent,
        alternate,
        location: node.location,
        loop_context: node.loop_context,
    })
}

fn lower_optional_expression(
    node: ExpressionNode,
    class: ExpressionClassification,
    ctx: &mut LoweringContext,
) -> TemplateNode {
    let cond_id = register_expression_typed(
        "opt",
        class.optional_condition.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    let fragment = parse_jsx_to_nodes(
        &class.optional_fragment.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    TemplateNode::OptionalFragment(OptionalFragmentNode {
        condition: cond_id,
        fragment,
        location: node.location,
        loop_context: node.loop_context,
    })
}

fn lower_loop_expression(
    node: ExpressionNode,
    class: ExpressionClassification,
    ctx: &mut LoweringContext,
) -> TemplateNode {
    let source_id = register_expression_typed(
        "loop",
        class.loop_source.unwrap(),
        node.location.clone(),
        node.loop_context.clone(),
        ctx,
    );
    let item_var = class.loop_item_var.unwrap();
    let index_var = class.loop_index_var;
    let mut vars = node
        .loop_context
        .as_ref()
        .map(|c| c.variables.clone())
        .unwrap_or_default();
    vars.push(item_var.clone());
    if let Some(ref idx) = index_var {
        vars.push(idx.clone());
    }
    let body_ctx = Some(LoopContext {
        variables: vars,
        map_source: Some(source_id.clone()),
    });
    let body = parse_jsx_to_nodes(
        &class.loop_body.unwrap(),
        node.location.clone(),
        body_ctx.clone(),
        ctx,
    );
    TemplateNode::LoopFragment(LoopFragmentNode {
        source: source_id,
        item_var,
        index_var,
        body,
        location: node.location,
        loop_context: body_ctx,
    })
}

/// Global atomic counter for globally unique expression IDs
/// This ensures IDs are unique across all component lowering passes
static EXPRESSION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Get next unique expression ID
fn next_expression_id() -> u64 {
    EXPRESSION_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// Register expression with type prefix (cond, opt, loop) to avoid ID collisions
fn register_expression_typed(
    type_prefix: &str,
    code: String,
    location: SourceLocation,
    loop_context: Option<LoopContext>,
    ctx: &mut LoweringContext,
) -> String {
    let id = format!("{}_{}", type_prefix, next_expression_id());
    ctx.expressions.push(ExpressionIR {
        id: id.clone(),
        code,
        location,
        loop_context,
    });
    id
}

/// Register general expression (text bindings inside JSX)
fn register_expression(
    code: String,
    location: SourceLocation,
    loop_context: Option<LoopContext>,
    ctx: &mut LoweringContext,
) -> String {
    let id = format!("expr_frag_{}", next_expression_id());
    ctx.expressions.push(ExpressionIR {
        id: id.clone(),
        code,
        location,
        loop_context,
    });
    id
}

fn parse_jsx_to_nodes(
    code: &str,
    loc: SourceLocation,
    lctx: Option<LoopContext>,
    ctx: &mut LoweringContext,
) -> Vec<TemplateNode> {
    let trimmed = code.trim();
    if trimmed.starts_with("<>") {
        let content = if let Some(idx) = trimmed[2..].rfind("</>") {
            &trimmed[2..2 + idx]
        } else {
            &trimmed[2..]
        };
        return parse_jsx_children(content, loc, lctx, ctx);
    }
    if trimmed.starts_with("<") {
        if let Some((node, _)) = parse_jsx_element_with_end(trimmed, loc.clone(), lctx.clone(), ctx)
        {
            return vec![node];
        }
    }
    if trimmed.starts_with("(") && trimmed.ends_with(")") {
        return parse_jsx_to_nodes(&trimmed[1..trimmed.len() - 1].trim(), loc, lctx, ctx);
    }
    vec![TemplateNode::Expression(ExpressionNode {
        expression: trimmed.to_string(),
        location: loc,
        loop_context: lctx,
    })]
}

fn parse_jsx_children(
    content: &str,
    loc: SourceLocation,
    lctx: Option<LoopContext>,
    ctx: &mut LoweringContext,
) -> Vec<TemplateNode> {
    let mut nodes = Vec::new();
    let mut i = 0;
    let mut text = String::new();
    while i < content.len() {
        let c_char = content[i..].chars().next().unwrap();
        let c_len = c_char.len_utf8();

        if c_char == '<'
            && i + 1 < content.len()
            && (content.as_bytes()[i + 1] as char).is_ascii_alphabetic()
        {
            if !text.trim().is_empty() {
                nodes.push(TemplateNode::Text(TextNode {
                    value: text.trim().to_string(),
                    location: loc.clone(),
                    loop_context: lctx.clone(),
                }));
                text.clear();
            }
            if let Some((node, end)) =
                parse_jsx_element_with_end(&content[i..], loc.clone(), lctx.clone(), ctx)
            {
                nodes.push(node);
                i += end;
                continue;
            }
        }
        if c_char == '{' {
            if let Some(end) = find_balanced_brace_end(&content[i..]) {
                if !text.trim().is_empty() {
                    nodes.push(TemplateNode::Text(TextNode {
                        value: text.trim().to_string(),
                        location: loc.clone(),
                        loop_context: lctx.clone(),
                    }));
                    text.clear();
                }
                let expr = content[i + 1..i + end - 1].trim();
                if !expr.is_empty() && !(expr.starts_with("/*") && expr.ends_with("*/")) {
                    let id = register_expression(expr.to_string(), loc.clone(), lctx.clone(), ctx);
                    nodes.push(TemplateNode::Expression(ExpressionNode {
                        expression: id,
                        location: loc.clone(),
                        loop_context: lctx.clone(),
                    }));
                }
                i += end;
                continue;
            }
        }
        text.push(c_char);
        i += c_len;
    }
    if !text.trim().is_empty() {
        nodes.push(TemplateNode::Text(TextNode {
            value: text.trim().to_string(),
            location: loc,
            loop_context: lctx,
        }));
    }
    nodes
}

fn parse_jsx_element_with_end(
    code: &str,
    loc: SourceLocation,
    lctx: Option<LoopContext>,
    ctx: &mut LoweringContext,
) -> Option<(TemplateNode, usize)> {
    lazy_static! {
        static ref TAG_RE: Regex = Regex::new(r"^<([a-zA-Z][a-zA-Z0-9.]*)").unwrap();
        static ref ATTR_RE: Regex = Regex::new(r"^([a-zA-Z_][a-zA-Z0-9_-]*)").unwrap();
    }
    let tag_caps = TAG_RE.captures(code)?;
    let tag = tag_caps.get(1)?.as_str().to_string();
    let mut i = tag_caps.get(0)?.end();
    let mut attrs = Vec::new();
    while i < code.len() {
        while i < code.len() && code.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= code.len() {
            break;
        }
        if code.as_bytes()[i] == b'>' {
            i += 1;
            break;
        }
        if code.as_bytes()[i] == b'/' && i + 1 < code.len() && code.as_bytes()[i + 1] == b'>' {
            let is_comp = if let Some(c) = tag.chars().next() {
                c.is_uppercase()
            } else {
                false
            };
            let node = if is_comp {
                TemplateNode::Component(ComponentNode {
                    name: tag,
                    attributes: attrs,
                    children: Vec::new(),
                    location: loc,
                    loop_context: lctx,
                })
            } else {
                TemplateNode::Element(ElementNode {
                    tag: tag.to_lowercase(),
                    attributes: attrs,
                    children: Vec::new(),
                    location: loc,
                    loop_context: lctx,
                })
            };
            return Some((node, i + 2));
        }
        if let Some(attr_caps) = ATTR_RE.captures(&code[i..]) {
            let name = attr_caps.get(1)?.as_str().to_string();
            i += attr_caps.get(0)?.end();
            while i < code.len() && code.as_bytes()[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < code.len() && code.as_bytes()[i] == b'=' {
                i += 1;
                while i < code.len() && code.as_bytes()[i].is_ascii_whitespace() {
                    i += 1;
                }
                if i < code.len() && (code.as_bytes()[i] == b'"' || code.as_bytes()[i] == b'\'') {
                    let q = code.as_bytes()[i];
                    let mut e = i + 1;
                    while e < code.len() && code.as_bytes()[e] != q {
                        if code.as_bytes()[e] == b'\\' {
                            e += 1;
                        }
                        e += 1;
                    }
                    if e < code.len() {
                        attrs.push(AttributeIR {
                            name,
                            value: serde_json::from_str(&format!(
                                "{{\"static\": \"{}\"}}",
                                &code[i + 1..e]
                            ))
                            .unwrap_or(AttributeValue::Static(code[i + 1..e].to_string())),
                            location: loc.clone(),
                            loop_context: lctx.clone(),
                        });
                        i = e + 1;
                    }
                } else if i < code.len() && code.as_bytes()[i] == b'{' {
                    if let Some(end) = find_balanced_brace_end(&code[i..]) {
                        let expr = code[i + 1..i + end - 1].trim().to_string();
                        let id = register_expression(expr.clone(), loc.clone(), lctx.clone(), ctx);
                        attrs.push(AttributeIR {
                            name,
                            value: AttributeValue::Dynamic(ExpressionIR {
                                id,
                                code: expr,
                                location: loc.clone(),
                                loop_context: lctx.clone(),
                            }),
                            location: loc.clone(),
                            loop_context: lctx.clone(),
                        });
                        i += end;
                    }
                }
            } else {
                attrs.push(AttributeIR {
                    name,
                    value: AttributeValue::Static("true".to_string()),
                    location: loc.clone(),
                    loop_context: lctx.clone(),
                });
            }
        } else {
            i += 1;
        }
    }
    let close = format!("</{}>", tag);
    if let Some(idx) = find_closing_tag(&code[i..], &tag) {
        let child_content = &code[i..i + idx];
        let children = parse_jsx_children(child_content, loc.clone(), lctx.clone(), ctx);
        i += idx + close.len();
        let is_comp = if let Some(c) = tag.chars().next() {
            c.is_uppercase()
        } else {
            false
        };
        let node = if is_comp {
            TemplateNode::Component(ComponentNode {
                name: tag,
                attributes: attrs,
                children,
                location: loc,
                loop_context: lctx,
            })
        } else {
            TemplateNode::Element(ElementNode {
                tag: tag.to_lowercase(),
                attributes: attrs,
                children,
                location: loc,
                loop_context: lctx,
            })
        };
        return Some((node, i));
    }
    None
}

fn find_closing_tag(code: &str, tag: &str) -> Option<usize> {
    let close = format!("</{}>", tag);
    let open_re = Regex::new(&format!(r"^<{}(?:\s|>|/>)", tag)).unwrap();
    let self_re = Regex::new(&format!(r"^<{}[^>]*/>", tag)).unwrap();
    let mut depth = 1;
    let mut i = 0;
    while i < code.len() && depth > 0 {
        if code[i..].starts_with(&close) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
            i += close.len();
            continue;
        }
        if let Some(m) = open_re.find(&code[i..]) {
            if !self_re.is_match(&code[i..i + m.end()]) {
                depth += 1;
            }
            i += m.end();
            continue;
        }
        if let Some(c) = code[i..].chars().next() {
            i += c.len_utf8();
        } else {
            break;
        }
    }
    None
}

fn find_balanced_brace_end(code: &str) -> Option<usize> {
    if !code.starts_with('{') {
        return None;
    }
    let mut depth = 1;
    let mut i = 1;
    let mut in_s = false;
    let mut s_c = ' ';
    let b = code.as_bytes();
    while i < b.len() && depth > 0 {
        let c = b[i] as char;
        if i > 0 && b[i - 1] as char == '\\' {
            i += 1;
            continue;
        }
        if !in_s && (c == '"' || c == '\'') {
            in_s = true;
            s_c = c;
            i += 1;
            continue;
        }
        if in_s && c == s_c {
            in_s = false;
            i += 1;
            continue;
        }
        if !in_s {
            if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
            }
        }
        i += 1;
    }
    if depth == 0 {
        Some(i)
    } else {
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// NAPI WRAPPERS (LEGACY SUPPORT)
// ═══════════════════════════════════════════════════════════════════════════════

#[napi]
pub fn evaluate_expression_native(
    id: String,
    code: String,
    known_bindings_json: String,
    loop_context_json: Option<String>,
) -> EvaluatedExpression {
    let known: HashSet<String> = serde_json::from_str(&known_bindings_json).unwrap_or_default();
    let lc: Option<LoopContextInput> =
        loop_context_json.and_then(|s| serde_json::from_str(&s).ok());

    let classification = classify_expression(&code);
    let (deps, uses_state, uses_loop_context) = compute_dependencies(&code, &known, &lc);

    EvaluatedExpression {
        id,
        expression_type: format!("{:?}", classification.expr_type).to_lowercase(),
        dependencies: deps,
        uses_state,
        uses_loop_context,
        classification_json: serde_json::to_string(&classification).unwrap_or_default(),
    }
}

#[napi]
pub fn classify_expression_native(code: String) -> String {
    let classification = classify_expression(&code);
    serde_json::to_string(&classification).unwrap_or_default()
}

#[napi]
pub fn analyze_expressions(input_json: String) -> AnalysisOutput {
    let input: AnalysisInput = match serde_json::from_str(&input_json) {
        Ok(parsed) => parsed,
        Err(_) => {
            return AnalysisOutput {
                results: vec![],
                binding_count: 0,
            }
        }
    };
    let known: HashSet<String> = input.known_bindings.into_iter().collect();
    let mut results = Vec::new();
    for expr in input.expressions {
        let class = classify_expression(&expr.code);
        let lc = expr.loop_context.map(|l| LoopContextInput {
            variables: l.variables,
            map_source: l.map_source,
        });
        let (deps, state, lctx_uses) = compute_dependencies(&expr.code, &known, &lc);
        results.push(ExpressionAnalysisResult {
            id: expr.id,
            classification: serde_json::to_string(&class).unwrap_or_default(),
            dependencies: deps,
            uses_state: state,
            uses_loop_context: lctx_uses,
        });
    }
    AnalysisOutput {
        results,
        binding_count: known.len() as u32,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct Binding {
    pub id: String,
    pub r#type: String, // 'text' | 'attribute' | 'conditional' | 'optional' | 'loop'
    pub target: String,
    pub expression: String,
    pub location: Option<SourceLocation>,
    pub loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[napi(object)]
pub struct TransformOutput {
    pub html: String,
    pub bindings: Vec<Binding>,
}

#[napi]
pub fn transform_template_native(
    nodes_json: String,
    expressions_json: String,
) -> napi::Result<TransformOutput> {
    let nodes: Vec<TemplateNode> = serde_json::from_str(&nodes_json)
        .map_err(|e| napi::Error::from_reason(format!("Nodes parse error: {}", e)))?;
    let expressions: Vec<ExpressionIR> = serde_json::from_str(&expressions_json)
        .map_err(|e| napi::Error::from_reason(format!("Expressions parse error: {}", e)))?;

    let mut html = String::new();
    let mut bindings = Vec::new();

    for node in nodes {
        let (node_html, node_bindings) = transform_node_internal(&node, &expressions, &None, false);
        html.push_str(&node_html);
        bindings.extend(node_bindings);
    }

    Ok(TransformOutput { html, bindings })
}

fn transform_node_internal(
    node: &TemplateNode,
    expressions: &[ExpressionIR],
    parent_loop_context: &Option<LoopContext>,
    is_inside_head: bool,
) -> (String, Vec<Binding>) {
    let mut bindings = Vec::new();

    let html = match node {
        TemplateNode::Text(t) => escape_html(&t.value),

        TemplateNode::Doctype(doc) => {
            let mut content = format!("<!DOCTYPE {}", doc.name);
            if !doc.public_id.is_empty() {
                content.push_str(&format!(" PUBLIC \"{}\"", doc.public_id));
            }
            if !doc.system_id.is_empty() {
                content.push_str(&format!(" \"{}\"", doc.system_id));
            }
            content.push('>');
            content
        }

        TemplateNode::Expression(expr_node) => {
            let expr = expressions
                .iter()
                .find(|e| e.id == expr_node.expression)
                .expect("Expression not found");

            let active_loop_context = expr_node
                .loop_context
                .clone()
                .or(parent_loop_context.clone());

            bindings.push(Binding {
                id: expr.id.clone(),
                r#type: "text".to_string(),
                target: "data-zen-text".to_string(),
                expression: expr.code.clone(),
                location: Some(expr.location.clone()),
                loop_context: active_loop_context,
            });

            format!(
                "<span data-zen-text=\"{}\" style=\"display: contents;\"></span>",
                expr.id
            )
        }

        TemplateNode::Element(el) => {
            let tag = &el.tag;
            let mut attrs = Vec::new();

            for attr in &el.attributes {
                match &attr.value {
                    AttributeValue::Static(v) => {
                        attrs.push(format!("{}=\"{}\"", attr.name, escape_html(v)));
                    }
                    AttributeValue::Dynamic(expr) => {
                        let active_loop_context =
                            attr.loop_context.clone().or(parent_loop_context.clone());

                        bindings.push(Binding {
                            id: expr.id.clone(),
                            r#type: "attribute".to_string(),
                            target: attr.name.clone(),
                            expression: expr.code.clone(),
                            location: Some(expr.location.clone()),
                            loop_context: active_loop_context,
                        });

                        attrs.push(format!("data-zen-attr-{}={}", attr.name, expr.id));
                    }
                }
            }

            let attr_str = if attrs.is_empty() {
                "".to_string()
            } else {
                format!(" {}", attrs.join(" "))
            };

            let active_loop_context = el.loop_context.clone().or(parent_loop_context.clone());
            let next_in_head = is_inside_head || tag.to_lowercase() == "head";

            let mut children_html = String::new();
            for child in &el.children {
                let (c_html, c_bindings) =
                    transform_node_internal(child, expressions, &active_loop_context, next_in_head);
                children_html.push_str(&c_html);
                bindings.extend(c_bindings);
            }

            let void_elements: HashSet<&str> = [
                "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta",
                "param", "source", "track", "wbr",
            ]
            .iter()
            .cloned()
            .collect();

            if void_elements.contains(tag.to_lowercase().as_str()) && children_html.is_empty() {
                format!("<{}{} />", tag, attr_str)
            } else {
                format!("<{}{}>{}</{}>", tag, attr_str, children_html, tag)
            }
        }

        TemplateNode::ConditionalFragment(cond) => {
            let expr = expressions
                .iter()
                .find(|e| e.id == cond.condition)
                .expect("Condition expression not found");

            bindings.push(Binding {
                id: expr.id.clone(),
                r#type: "conditional".to_string(),
                target: "data-zen-conditional".to_string(),
                expression: expr.code.clone(),
                location: Some(expr.location.clone()),
                loop_context: cond.loop_context.clone(),
            });

            let mut cons_html = String::new();
            for child in &cond.consequent {
                let (c_html, c_bindings) =
                    transform_node_internal(child, expressions, &cond.loop_context, is_inside_head);
                cons_html.push_str(&c_html);
                bindings.extend(c_bindings);
            }

            let mut alt_html = String::new();
            for child in &cond.alternate {
                let (a_html, a_bindings) =
                    transform_node_internal(child, expressions, &cond.loop_context, is_inside_head);
                alt_html.push_str(&a_html);
                bindings.extend(a_bindings);
            }

            format!(
                "<div data-zen-conditional=\"{}\" style=\"display: contents;\">\n<div data-zen-branch=\"true\" style=\"display: contents;\">{}</div>\n<div data-zen-branch=\"false\" style=\"display: contents;\">{}</div>\n</div>",
                expr.id, cons_html, alt_html
            )
        }

        TemplateNode::OptionalFragment(opt) => {
            let expr = expressions
                .iter()
                .find(|e| e.id == opt.condition)
                .expect("Optional condition expression not found");

            bindings.push(Binding {
                id: expr.id.clone(),
                r#type: "optional".to_string(),
                target: "data-zen-optional".to_string(),
                expression: expr.code.clone(),
                location: Some(expr.location.clone()),
                loop_context: opt.loop_context.clone(),
            });

            let mut frag_html = String::new();
            for child in &opt.fragment {
                let (c_html, c_bindings) =
                    transform_node_internal(child, expressions, &opt.loop_context, is_inside_head);
                frag_html.push_str(&c_html);
                bindings.extend(c_bindings);
            }

            format!(
                "<div data-zen-optional=\"{}\" style=\"display: contents;\">{}</div>",
                expr.id, frag_html
            )
        }

        TemplateNode::LoopFragment(lp) => {
            let expr = expressions
                .iter()
                .find(|e| e.id == lp.source)
                .expect("Loop source expression not found");

            bindings.push(Binding {
                id: expr.id.clone(),
                r#type: "loop".to_string(),
                target: "data-zen-loop".to_string(),
                expression: expr.code.clone(),
                location: Some(expr.location.clone()),
                loop_context: lp.loop_context.clone(),
            });

            let mut body_html = String::new();
            for child in &lp.body {
                let (b_html, b_bindings) =
                    transform_node_internal(child, expressions, &lp.loop_context, is_inside_head);
                body_html.push_str(&b_html);
                bindings.extend(b_bindings);
            }

            let index_attr = if let Some(ref idx) = lp.index_var {
                format!(" data-zen-index=\"{}\"", idx)
            } else {
                "".to_string()
            };

            format!(
                "<template data-zen-loop=\"{}\" data-zen-item=\"{}\"{}>{}</template>",
                expr.id, lp.item_var, index_attr, body_html
            )
        }

        TemplateNode::Component(comp) => {
            let mut children_html = String::new();
            for child in &comp.children {
                let (c_html, c_bindings) =
                    transform_node_internal(child, expressions, &comp.loop_context, is_inside_head);
                children_html.push_str(&c_html);
                bindings.extend(c_bindings);
            }
            format!(
                "<div data-zen-component=\"{}\" style=\"display: contents;\">{}</div>",
                comp.name, children_html
            )
        }
    };

    (html, bindings)
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&#39;")
}
