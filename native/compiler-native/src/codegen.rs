//! Codegen module for Zenith compiler
//!
//! Generates runtime JavaScript code from ZenIR input.
//! This is the Rust authority for all compilation - no TypeScript fallback.

use crate::jsx_lowerer::{JsxLowerer, ScriptRenamer};
use crate::validate::{AttributeValue, ElementNode, ExpressionInput, StyleIR, TemplateNode};
use napi_derive::napi;
use oxc_allocator::Allocator;
use oxc_ast::{ast::*, AstBuilder};
use oxc_ast_visit::VisitMut;
use oxc_codegen::Codegen;
use oxc_parser::Parser;
use oxc_span::{SourceType, SPAN};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════════════════
// INPUT/OUTPUT TYPES
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodegenInput {
    pub file_path: String,
    pub script_content: String,
    pub expressions: Vec<ExpressionInput>,
    pub styles: Vec<StyleIR>,
    pub template_bindings: Vec<String>,
    pub location: String,
    pub nodes: Vec<TemplateNode>,
    #[serde(default)]
    pub page_bindings: Vec<String>, // Page-level state bindings (extracted before component inlining)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StateDeclaration {
    pub name: String,
    pub initial_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCode {
    pub expressions: String,
    pub render: String,
    pub hydration: String,
    pub styles: String,
    pub script: String,
    pub state_init: String,
    pub bundle: String,
    pub npm_imports: Vec<ScriptImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct ScriptImport {
    pub source: String,
    pub specifiers: String,
    pub type_only: bool,
    pub side_effect: bool,
}

// ═══════════════════════════════════════════════════════════════════════════════
// NAPI EXPORT
// ═══════════════════════════════════════════════════════════════════════════════

#[napi]
pub fn generate_runtime_code(input_json: String) -> napi::Result<RuntimeCode> {
    let input: CodegenInput = serde_json::from_str(&input_json)
        .map_err(|e| napi::Error::from_reason(format!("Failed to parse input: {}", e)))?;

    Ok(generate_runtime_code_internal(input))
}

#[napi]
pub fn generate_codegen_intent() -> String {
    "Rust Codegen Authority".to_string()
}

// ═══════════════════════════════════════════════════════════════════════════════
// INTERNAL IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════════

pub fn generate_runtime_code_internal(input: CodegenInput) -> RuntimeCode {
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_jsx(true).with_typescript(true);

    // 1. Replace "state " with "let " for parsing
    // Only match 'state' at statement boundaries (start, newline, semicolon, braces)
    // Avoid matching 'state' in comments or strings
    let state_re = Regex::new(r"(^|[\n;{}])\s*state\s+").unwrap();
    let parsable_script = state_re
        .replace_all(&input.script_content, "$1 let ")
        .to_string();

    // 2. Extract state bindings using Regex (more robust than parsing substituted script)
    // Only match 'state' at statement boundaries to avoid comments
    let mut bindings = HashSet::new();
    let state_decl_re = Regex::new(r"(?:^|[\n;{}])\s*state\s+([a-zA-Z_$][a-zA-Z0-9_$]*)").unwrap();
    for cap in state_decl_re.captures_iter(&input.script_content) {
        bindings.insert(cap[1].to_string());
    }

    // Merge page-level bindings (extracted BEFORE component inlining)
    for pb in &input.page_bindings {
        bindings.insert(pb.clone());
    }
    println!(
        "[ZenithNative] page_bindings from TS: {:?}",
        input.page_bindings
    );
    println!("[ZenithNative] merged bindings: {:?}", bindings);

    let mut state_decls = Vec::new();
    let parser = Parser::new(&allocator, &parsable_script, source_type);
    let ret = parser.parse();

    if !ret.errors.is_empty() {
        println!("[ZenithNative] Parser Errors: {:?}", ret.errors);
    }

    println!(
        "[ZenithNative] Searching for state decls in {} statements",
        ret.program.body.len()
    );
    let mut found_bindings = HashSet::new();
    for stmt in &ret.program.body {
        if let Statement::VariableDeclaration(var_decl) = stmt {
            for decl in &var_decl.declarations {
                if let BindingPattern::BindingIdentifier(id) = &decl.id {
                    let name = id.name.to_string();
                    let in_bindings = bindings.contains(&name);
                    println!(
                        "[ZenithNative] Found var decl '{}', in_bindings={}",
                        name, in_bindings
                    );
                    if in_bindings {
                        println!("[ZenithNative] State decl '{}'", name);
                        found_bindings.insert(name.clone());
                        state_decls.push(StateDeclaration {
                            name: name.clone(),
                            initial_value: "undefined".to_string(),
                        });
                    }
                }
            }
        }
    }

    // Fallback: use regex to find any bindings that AST missed
    // This handles cases where state declarations are deeper in the script after component merging
    for binding in &bindings {
        if !found_bindings.contains(binding) && binding != "state" {
            // Try to find 'state BINDING = VALUE' or 'let BINDING = VALUE' in original/parsable script
            let pattern = format!(r"(?:state|let)\s+{}\s*=\s*([^;]+)", regex::escape(binding));
            if let Ok(re) = Regex::new(&pattern) {
                if let Some(cap) = re.captures(&input.script_content) {
                    let initial_value = "undefined".to_string();
                    println!("[ZenithNative] Regex fallback: '{}'", binding);
                    state_decls.push(StateDeclaration {
                        name: binding.clone(),
                        initial_value,
                    });
                } else {
                    // No init found, use undefined
                    println!(
                        "[ZenithNative] Binding '{}' not found in script, using undefined",
                        binding
                    );
                    state_decls.push(StateDeclaration {
                        name: binding.clone(),
                        initial_value: "undefined".to_string(),
                    });
                }
            }
        }
    }
    println!("[ZenithNative] state_bindings (regex): {:?}", bindings);

    // 3. Transform script with identifier renaming and HOIST IMPORTS
    let parser = Parser::new(&allocator, &parsable_script, source_type);
    let mut program = parser.parse().program;
    let ast = AstBuilder::new(&allocator);

    // Separate imports from body
    let mut body = ast.vec();
    let mut import_lines = Vec::new();
    let mut seen_imports = HashSet::new();
    let mut script_imports = Vec::new();

    for stmt in program.body.into_iter() {
        if let Statement::ImportDeclaration(mut import_decl) = stmt {
            // Fix .zen extensions here too
            let source = import_decl.source.value.to_string();
            let final_source = if source.ends_with(".zen") {
                source.replace(".zen", ".js")
            } else {
                source
            };

            // Capture info BEFORE moving import_decl
            let is_type = import_decl.import_kind.is_type();
            let is_side_effect = import_decl.specifiers.is_none();
            let source_for_struct = final_source.clone();

            if final_source != import_decl.source.value.to_string() {
                import_decl.source.value = allocator.alloc_str(&final_source).into();
            }

            let import_code = Codegen::new()
                .build(&Program {
                    span: SPAN,
                    source_type,
                    hashbang: None,
                    directives: ast.vec(),
                    body: {
                        let mut b = ast.vec();
                        b.push(Statement::ImportDeclaration(import_decl));
                        b
                    },
                    source_text: "",
                    comments: ast.vec(),
                    scope_id: std::cell::Cell::new(None),
                })
                .code;

            let trimmed_import = import_code.trim().to_string();
            if !seen_imports.contains(&trimmed_import) {
                seen_imports.insert(trimmed_import.clone());
                import_lines.push(import_code);

                // Extract specifiers using regex from the generated import_code
                // This is more robust than manual extraction for now
                let spec_re = Regex::new(r"import\s+(.*?)\s+from").unwrap();
                let specifiers = if let Some(cap) = spec_re.captures(&trimmed_import) {
                    cap.get(1)
                        .map_or("".to_string(), |m| m.as_str().to_string())
                } else {
                    "".to_string()
                };

                // Collect structured import for the bridge
                script_imports.push(ScriptImport {
                    source: source_for_struct,
                    specifiers,
                    type_only: is_type,
                    side_effect: is_side_effect,
                });
            }
        } else {
            body.push(stmt);
        }
    }
    program.body = body;

    println!(
        "[ZenithNative] Script transformation start for {}...",
        input.file_path
    );
    let mut renamer = ScriptRenamer::new(&allocator, bindings.clone());
    renamer.visit_program(&mut program);
    println!("[ZenithNative] Script transformation end.");
    let script_no_imports = Codegen::new().build(&program).code;

    let all_imports = import_lines.join("");

    // 4. Prepare state variables set for expression transformation
    let mut state_vars = bindings.clone();
    state_vars.insert("props".to_string());
    state_vars.insert("stores".to_string());
    state_vars.insert("loaderData".to_string());

    let loop_vars: HashSet<String> = input.template_bindings.iter().cloned().collect();

    // 5. Generate Template IR
    let template_ir = if input.nodes.is_empty() {
        "__zenith.fragment([])".to_string()
    } else if input.nodes.len() == 1 {
        generate_template_ir(&input.nodes[0], &input.expressions)
    } else {
        let child_irs: Vec<String> = input
            .nodes
            .iter()
            .map(|n| generate_template_ir(n, &input.expressions))
            .collect();
        format!("__zenith.fragment([{}])", child_irs.join(", "))
    };

    let render_fn = format!(
        "function renderDynamicPage(state) {{\n  return {};\n}}",
        template_ir
    );

    // 6. Generate Expression Wrappers
    let expressions_code = input
        .expressions
        .iter()
        .map(|expr| {
            let (transformed_code, _deps, uses_loop) =
                compute_expression_intent(expr, &state_vars, &loop_vars);

            let args = if uses_loop {
                "state, item, index, array, globalState"
            } else {
                "state"
            };

            let fn_name = format!("_expr_{}", expr.id);
            format!(
                "function {}({}) {{
  try {{
    const v = ({});
    return (v && typeof v === 'function' && v._isSignal) ? v() : v;
  }} catch (e) {{
    console.error('[Zenith Runtime] Expression {} failed:', e);
    return '';
  }}
}}",
                fn_name,
                args,
                transformed_code.trim_end_matches(';'),
                expr.id
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // 7. Expression Registry
    let expression_registry = if input.expressions.is_empty() {
        "// No expressions to register".to_string()
    } else {
        let entries: Vec<String> = input
            .expressions
            .iter()
            .map(|e| {
                format!(
                    "  window.__ZENITH_EXPRESSIONS__.set('{}', _expr_{});",
                    e.id, e.id
                )
            })
            .collect();
        format!(
            "if (typeof window !== 'undefined') {{\n  if (!window.__ZENITH_EXPRESSIONS__) window.__ZENITH_EXPRESSIONS__ = new Map();\n{}\n}}",
            entries.join("\n")
        )
    };

    // 8. Styles
    let styles_code = input
        .styles
        .iter()
        .map(|s| s.raw.clone())
        .collect::<Vec<_>>()
        .join("\n");

    // 9. State Init
    let state_init_code = state_decls
        .iter()
        .map(|d| format!("state.{} = {};", d.name, d.initial_value))
        .collect::<Vec<_>>()
        .join("\n");

    println!(
        "[ZenithNative] state_decls has {} entries:",
        state_decls.len()
    );
    for d in &state_decls {
        println!("[ZenithNative] - {} = {}", d.name, d.initial_value);
    }
    let state_props: Vec<String> = state_decls
        .iter()
        .map(|d| format!("  {}: {}", d.name, d.initial_value))
        .collect();

    let reactive_state_init = format!(
        "const state = typeof zenState === 'function' ? zenState({{\n{}\n}}) : {{\n{}\n}};",
        state_props.join(",\n"),
        state_props.join(",\n")
    );

    // 10. Hydration Runtime (embedded)
    let hydration = include_str!("hydration_runtime.js");

    // 11. Bundle construction
    let bundle = format!(
        "{}\n// [ZENITH-NATIVE] Rust Compiler Authority Bundle\n\n(function() {{\n  if (typeof window === 'undefined') return;\n\n  // 1. Zenith Runtime\n  {}\n\n  // 2. Component instance\n  const __instance = {{ mountHooks: [] }};\n  if (window.__zenith && window.__zenith.setActiveInstance) {{\n    window.__zenith.setActiveInstance(__instance);\n  }}\n\n  // 3. Reactive state\n  {}\n\n  // 4. User script\n  {}\n\n  // 5. Expressions\n  {}\n  {}\n\n  // 6. Styles injection\n  const __styles = `{}`;\n  if (__styles && typeof document !== 'undefined') {{\n    const styleTag = document.createElement('style');\n    styleTag.textContent = __styles;\n    document.head.appendChild(styleTag);\n  }}\n\n  // 7. Template IR\n  const canonicalIR = (state) => {{\n    return {};\n  }};\n  window.canonicalIR = canonicalIR;\n\n  // 8. Hydration\n  function initHydration() {{\n    if (typeof window.zenithHydrate === 'function') {{\n      window.zenithHydrate(state, document);\n    }}\n    if (window.__zenith && window.__zenith.triggerMount) {{\n      window.__zenith.triggerMount(__instance);\n    }}\n  }}\n\n  if (document.readyState === 'loading') {{\n    document.addEventListener('DOMContentLoaded', initHydration);\n  }} else {{\n    initHydration();\n  }}\n}})();\n",
        all_imports,
        hydration,
        reactive_state_init,
        script_no_imports,
        expressions_code,
        expression_registry,
        styles_code,
        template_ir
    );

    RuntimeCode {
        expressions: expressions_code,
        render: render_fn,
        hydration: hydration.to_string(),
        styles: styles_code,
        script: script_no_imports,
        state_init: state_init_code,
        bundle,
        npm_imports: script_imports,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEMPLATE IR GENERATION
// ═══════════════════════════════════════════════════════════════════════════════

fn get_node_args(node_loop_context: &Option<crate::validate::LoopContext>) -> String {
    if let Some(lc) = node_loop_context {
        if lc.variables.is_empty() {
            "state".to_string()
        } else {
            format!("state, {}", lc.variables.join(", "))
        }
    } else {
        "state".to_string()
    }
}

fn generate_template_ir(node: &TemplateNode, expressions: &[ExpressionInput]) -> String {
    match node {
        TemplateNode::Element(el) => generate_element_ir(el, expressions),
        TemplateNode::Text(t) => format!("\"{}\"", escape_js_string(&t.value)),
        TemplateNode::Expression(e) => {
            let expr_id = expressions
                .iter()
                .find(|ex| ex.code.trim() == e.expression.trim() || ex.id == e.expression)
                .map(|ex| ex.id.clone())
                .unwrap_or_else(|| format!("inline_{}", e.expression.len()));
            let args = get_node_args(&e.loop_context);
            format!("() => (_expr_{}({}))", expr_id, args)
        }
        TemplateNode::LoopFragment(loop_node) => {
            let body_ir: Vec<String> = loop_node
                .body
                .iter()
                .map(|n| generate_template_ir(n, expressions))
                .collect();
            let source_id = expressions
                .iter()
                .find(|ex| ex.code.trim() == loop_node.source.trim() || ex.id == loop_node.source)
                .map(|ex| ex.id.clone())
                .unwrap_or_else(|| loop_node.source.clone());

            // CRITICAL: The source expression should NOT receive loop variables that are
            // introduced BY this loop. Those variables (item_var, index_var) don't exist
            // until INSIDE the .map() callback. We need to filter them out.
            let parent_args = if let Some(ref lc) = loop_node.loop_context {
                // Filter out this loop's own variables from the context
                let parent_vars: Vec<&String> = lc
                    .variables
                    .iter()
                    .filter(|v| {
                        *v != &loop_node.item_var
                            && loop_node.index_var.as_ref().map_or(true, |idx| *v != idx)
                    })
                    .collect();
                if parent_vars.is_empty() {
                    "state".to_string()
                } else {
                    format!(
                        "state, {}",
                        parent_vars
                            .iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            } else {
                "state".to_string()
            };

            format!(
                "(_expr_{}({})).map(({}{}) => {})",
                source_id,
                parent_args,
                loop_node.item_var,
                loop_node
                    .index_var
                    .as_ref()
                    .map(|i| format!(", {}", i))
                    .unwrap_or_default(),
                if body_ir.len() == 1 {
                    body_ir[0].clone()
                } else {
                    format!("[{}]", body_ir.join(", "))
                }
            )
        }

        TemplateNode::ConditionalFragment(cond) => {
            let cons: Vec<String> = cond
                .consequent
                .iter()
                .map(|n| generate_template_ir(n, expressions))
                .collect();
            let alt: Vec<String> = cond
                .alternate
                .iter()
                .map(|n| generate_template_ir(n, expressions))
                .collect();
            let cond_id = expressions
                .iter()
                .find(|ex| ex.code.trim() == cond.condition.trim() || ex.id == cond.condition)
                .map(|ex| ex.id.clone())
                .unwrap_or_else(|| cond.condition.clone());
            let args = get_node_args(&cond.loop_context);
            format!(
                "(_expr_{}({})) ? {} : {}",
                cond_id,
                args,
                if cons.len() == 1 {
                    cons[0].clone()
                } else {
                    format!("[{}]", cons.join(", "))
                },
                if alt.len() == 1 {
                    alt[0].clone()
                } else {
                    format!("[{}]", alt.join(", "))
                }
            )
        }
        TemplateNode::OptionalFragment(opt) => {
            let frag: Vec<String> = opt
                .fragment
                .iter()
                .map(|n| generate_template_ir(n, expressions))
                .collect();
            let cond_id = expressions
                .iter()
                .find(|ex| ex.code.trim() == opt.condition.trim() || ex.id == opt.condition)
                .map(|ex| ex.id.clone())
                .unwrap_or_else(|| opt.condition.clone());
            let args = get_node_args(&opt.loop_context);
            format!(
                "(_expr_{}({})) && {}",
                cond_id,
                args,
                if frag.len() == 1 {
                    frag[0].clone()
                } else {
                    format!("[{}]", frag.join(", "))
                }
            )
        }
        TemplateNode::Component(c) => format!("/* Component {} */\"\"", c.name),
        TemplateNode::Doctype(_) => "\"\"".to_string(),
    }
}

fn generate_element_ir(el: &ElementNode, expressions: &[ExpressionInput]) -> String {
    let args = get_node_args(&el.loop_context);
    let props: Vec<String> = el
        .attributes
        .iter()
        .filter_map(|attr| {
            // Convert data-zen-* event handlers to on* function props
            let (prop_name, prop_val) = match attr.name.as_str() {
                "data-zen-click" => {
                    if let AttributeValue::Static(fn_name) = &attr.value {
                        // Convert to onclick function prop
                        ("onclick".to_string(), format!("() => {}()", fn_name))
                    } else {
                        return Some(format!("\"onclick\": () => {{}}"));
                    }
                }
                "data-zen-change" => {
                    if let AttributeValue::Static(fn_name) = &attr.value {
                        ("onchange".to_string(), format!("(e) => {}(e)", fn_name))
                    } else {
                        return Some(format!("\"onchange\": (e) => {{}}"));
                    }
                }
                "data-zen-input" => {
                    if let AttributeValue::Static(fn_name) = &attr.value {
                        ("oninput".to_string(), format!("(e) => {}(e)", fn_name))
                    } else {
                        return Some(format!("\"oninput\": (e) => {{}}"));
                    }
                }
                "data-zen-submit" => {
                    if let AttributeValue::Static(fn_name) = &attr.value {
                        (
                            "onsubmit".to_string(),
                            format!("(e) => {{ e.preventDefault(); {}(e); }}", fn_name),
                        )
                    } else {
                        return Some(format!("\"onsubmit\": (e) => {{}}"));
                    }
                }
                _ => {
                    // Standard attribute handling
                    let val = match &attr.value {
                        AttributeValue::Static(s) => format!("\"{}\"", escape_js_string(s)),
                        AttributeValue::Dynamic(expr) => {
                            format!("() => (_expr_{}({}))", expr.id, args)
                        }
                    };
                    (attr.name.clone(), val)
                }
            };
            Some(format!("\"{}\": {}", prop_name, prop_val))
        })
        .collect();

    if el.tag == "html" || el.tag == "body" {
        let child_irs: Vec<String> = el
            .children
            .iter()
            .map(|c| generate_template_ir(c, expressions))
            .collect();
        return format!("__zenith.fragment([{}])", child_irs.join(", "));
    }

    if el.tag == "head" {
        return "\"\"".to_string();
    }

    let props_str = if props.is_empty() {
        "null".to_string()
    } else {
        format!("{{ {} }}", props.join(", "))
    };

    let children: Vec<String> = el
        .children
        .iter()
        .map(|c| generate_template_ir(c, expressions))
        .collect();
    let children_str = format!("[{}]", children.join(", "));

    format!(
        "__zenith.h(\"{}\", {}, {})",
        el.tag, props_str, children_str
    )
}

// ═══════════════════════════════════════════════════════════════════════════════
// EXPRESSION INTENT
// ═══════════════════════════════════════════════════════════════════════════════

fn compute_expression_intent(
    expr: &ExpressionInput,
    state_bindings: &HashSet<String>,
    loop_vars: &HashSet<String>,
) -> (String, Vec<String>, bool) {
    let allocator = Allocator::default();
    let source_type = SourceType::default().with_jsx(true).with_typescript(true);
    let code = &expr.code;

    // Check if it uses loop variables (fast check)
    let uses_loop = expr.loop_context.is_some() || loop_vars.iter().any(|v| code.contains(v));

    // Parse the expression
    let parser = Parser::new(&allocator, code, source_type);
    let ret = parser.parse();
    if !ret.errors.is_empty() {
        // Fallback to original code if parsing fails (e.g. fragment bits)
        return (code.clone(), vec![], uses_loop);
    }

    let mut program = ret.program;

    // 1. Lower JSX to __zenith.h calls
    let mut jsx_lowerer = JsxLowerer::new(&allocator);
    jsx_lowerer.visit_program(&mut program);

    // 2. Rename identifiers (state.count, local vars)
    let mut locals: HashSet<String> = if let Some(lc) = &expr.loop_context {
        lc.variables.iter().cloned().collect()
    } else {
        HashSet::new()
    };
    for v in loop_vars {
        locals.insert(v.clone());
    }

    let mut renamer = ScriptRenamer::with_locals(&allocator, state_bindings.clone(), locals);
    renamer.visit_program(&mut program);

    // Codegen the transformed expression
    let mut transformed = Codegen::new().build(&program).code;
    // Trim trailing whitespace and SEMICOLONS (Expressions in Zenith should not have them internally)
    transformed = transformed.trim().trim_end_matches(';').to_string();

    // Collect dependencies (state variables actually used)
    let deps: Vec<String> = state_bindings
        .iter()
        .filter(|v| transformed.contains(&format!("state.{}", v)))
        .cloned()
        .collect();

    (transformed, deps, uses_loop)
}

fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_js_string() {
        assert_eq!(escape_js_string("hello\"world"), "hello\\\"world");
        assert_eq!(escape_js_string("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_expression_intent() {
        let expr = ExpressionInput {
            id: "test".to_string(),
            code: "count + 1".to_string(),
            loop_context: None,
        };
        let mut state_vars = HashSet::new();
        state_vars.insert("count".to_string());
        let loop_vars = HashSet::new();

        let (code, deps, uses_loop) = compute_expression_intent(&expr, &state_vars, &loop_vars);
        assert!(code.contains("state.count"));
        assert!(deps.contains(&"count".to_string()));
        assert!(!uses_loop);
    }
}
