use crate::validate::{ExpressionIR, LoopContext, TemplateNode, ZenIR};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentIR {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub template: String,
    #[serde(default)]
    pub nodes: Vec<TemplateNode>,
    #[serde(default)]
    pub expressions: Vec<ExpressionIR>,
    #[serde(default)]
    pub slots: Vec<SlotDefinition>,
    #[serde(default)]
    pub props: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub script: Option<String>,
    #[serde(default)]
    pub script_attributes: Option<HashMap<String, String>>,
    #[serde(default)]
    pub has_script: bool,
    #[serde(default)]
    pub has_styles: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotDefinition {
    pub name: Option<String>, // None = default slot
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSlots {
    pub default: Vec<TemplateNode>,
    pub named: HashMap<String, Vec<TemplateNode>>,
    pub parent_loop_context: Option<LoopContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

// Ensure SourceLocation matches validate.rs if needed, but validate.rs defines it too.
// If validte.rs defines SourceLocation, we should use it from there to avoid confusion?
// validate.rs has SourceLocation. Let's use `crate::validate::SourceLocation` and remove local def if possible?
// Local definition might be used by local structs.
// But wait, validate.rs `TemplateNode` uses `crate::validate::SourceLocation`.
// Our `ComponentIR` uses `SlotDefinition` which uses `SourceLocation`.
// If we remove local `SourceLocation`, we must import validte `SourceLocation`.
// But `SlotDefinition` is local.
// Let's keep duplicate for now or better, use imported one.
// Actually, let's just stick to fixing imports for now.

use napi_derive::napi;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, PropertyKey, Statement};
use oxc_parser::Parser;
use oxc_span::SourceType;

// ... existing structs ...

#[napi]
#[derive(Default)]
struct ResolutionContext {
    used_components: HashSet<String>,
    instance_counter: u32,
    collected_expressions: Vec<ExpressionIR>,
    components: HashMap<String, ComponentIR>,
    merged_script: String,
}

#[napi]
pub fn resolve_components_native(ir_json: String, components_json: String) -> String {
    let mut ir: ZenIR = serde_json::from_str(&ir_json).expect("Failed to parse IR");
    let components_map: HashMap<String, ComponentIR> =
        serde_json::from_str(&components_json).expect("Failed to parse components");

    let mut ctx = ResolutionContext {
        components: components_map,
        ..Default::default()
    };

    // Accumulate existing script
    if let Some(script) = &ir.script {
        ctx.merged_script = script.raw.clone();
    }

    // Resolve nodes
    let resolved_nodes = resolve_nodes(ir.template.nodes, &mut ctx, 0);

    ir.template.nodes = resolved_nodes;

    // Append collected expressions
    ir.template.expressions.extend(ctx.collected_expressions);

    // Collect styles from components
    let mut component_styles = Vec::new();
    for name in &ctx.used_components {
        if let Some(comp) = ctx.components.get(name) {
            for style in &comp.styles {
                component_styles.push(crate::validate::StyleIR { raw: style.clone() });
            }
        }
    }
    ir.styles.extend(component_styles);

    // Update script - handle pages with no script initial tag
    if let Some(script) = &mut ir.script {
        script.raw = ctx.merged_script;
    } else if !ctx.merged_script.is_empty() {
        ir.script = Some(crate::validate::ScriptIR {
            raw: ctx.merged_script,
            attributes: HashMap::new(),
        });
    }

    serde_json::to_string(&ir).expect("Failed to serialize IR")
}

fn resolve_nodes(
    nodes: Vec<TemplateNode>,
    ctx: &mut ResolutionContext,
    depth: u32,
) -> Vec<TemplateNode> {
    let mut resolved = Vec::new();
    for node in nodes {
        match node {
            TemplateNode::Component(comp) => {
                resolved.extend(resolve_component_node(comp, ctx, depth));
            }
            TemplateNode::Element(mut elem) => {
                elem.children = resolve_nodes(elem.children, ctx, depth + 1);
                resolved.push(TemplateNode::Element(elem));
            }
            TemplateNode::ConditionalFragment(mut cond) => {
                cond.consequent = resolve_nodes(cond.consequent, ctx, depth + 1);
                cond.alternate = resolve_nodes(cond.alternate, ctx, depth + 1);
                resolved.push(TemplateNode::ConditionalFragment(cond));
            }
            TemplateNode::OptionalFragment(mut opt) => {
                opt.fragment = resolve_nodes(opt.fragment, ctx, depth + 1);
                resolved.push(TemplateNode::OptionalFragment(opt));
            }
            TemplateNode::LoopFragment(mut lp) => {
                lp.body = resolve_nodes(lp.body, ctx, depth + 1);
                resolved.push(TemplateNode::LoopFragment(lp));
            }
            _ => resolved.push(node),
        }
    }
    resolved
}

fn resolve_component_node(
    node: crate::validate::ComponentNode,
    ctx: &mut ResolutionContext,
    depth: u32,
) -> Vec<TemplateNode> {
    let mut name = node.name.clone();

    // Invariants check: try exact match first, then case-insensitive
    if !ctx.components.contains_key(&name) {
        let lower_name = name.to_lowercase();
        let mut found = false;
        for (comp_name, _) in &ctx.components {
            if comp_name.to_lowercase() == lower_name {
                name = comp_name.clone();
                found = true;
                break;
            }
        }

        if !found {
            // BUG FIX: If the component isn't in the registry (e.g. it's a Layout tag),
            // we MUST still resolve its children, otherwise the page content is lost.
            let mut unresolved_node = node.clone();
            unresolved_node.children = resolve_nodes(node.children, ctx, depth + 1);
            return vec![TemplateNode::Component(unresolved_node)];
        }
    }

    ctx.used_components.insert(name.clone());
    let comp = ctx.components.get(&name).unwrap().clone();

    // 1. Extract slots
    let slots = extract_slots(&name, node.children, node.loop_context.clone());

    // 2. Clone and rename logic
    let instance_id = ctx.instance_counter;
    ctx.instance_counter += 1;
    let instance_suffix = format!("inst{}", instance_id);

    let mut local_rename_map = HashMap::new();

    // Derive local symbols from script if present
    // Note: We need to parse script to get local declarations if we want strict safety
    // For now, if passed via ComponentIR we can use it?
    // ComponentIR has `script`.
    if let Some(script_content) = &comp.script {
        // We should parse declarations.
        // For now, let's assume we rename EVERYTHING in the map passed by JS or just discover?
        // JS logic: getLocalDeclarations(script).
        // We should implement get_local_declarations in Rust too using oxc.
        let locals = get_local_declarations(script_content);
        for local in locals {
            if !comp.props.contains(&local) {
                local_rename_map.insert(local.clone(), format!("{}_{}", local, instance_suffix));
            }
        }
    }

    // Props substitution
    let mut prop_substitution_map = HashMap::new();
    for attr in &node.attributes {
        let replacement = match &attr.value {
            crate::validate::AttributeValue::Static(s) => format!("\"{}\"", s), // Naive stringification
            crate::validate::AttributeValue::Dynamic(expr) => format!("({})", expr.code),
        };
        prop_substitution_map.insert(attr.name.clone(), replacement.clone());
        prop_substitution_map.insert(format!("props.{}", attr.name), replacement);
    }

    let mut unified_rename_map = local_rename_map.clone();
    unified_rename_map.extend(prop_substitution_map);

    let mut expression_id_map = HashMap::new();

    // 3. Promote Expressions
    for expr in &comp.expressions {
        let new_id = format!("{}_{}", expr.id, instance_suffix);
        expression_id_map.insert(expr.id.clone(), new_id.clone());
        let renamed_code = rename_symbols_safe(&expr.code, &unified_rename_map);

        ctx.collected_expressions.push(ExpressionIR {
            id: new_id,
            code: renamed_code,
            location: expr.location.clone(),
            loop_context: expr.loop_context.clone(), // Should we merge loop context here?
                                                     // Component expressions effectively "hoisted" but they run in component scope.
                                                     // When we inline, the code is renamed.
                                                     // The loop context of the component usage site isn't relevant for the internal expression definition
                                                     // UNLESS the prop passed in was using a loop var.
                                                     // But here we are processing the *component's defined expressions*.
                                                     // Their loop context is strictly internal to them.
        });
    }

    // 4. Merge Script
    if let Some(script_content) = &comp.script {
        let renamed_script = rename_symbols_safe(script_content, &unified_rename_map);
        ctx.merged_script.push_str("\n\n");
        ctx.merged_script.push_str(&renamed_script);
    }

    // 5. Expand Template
    // Need to clone nodes first as we are mutating
    let mut template_nodes = comp.nodes.clone();

    rewrite_node_expressions(&mut template_nodes, &expression_id_map, &unified_rename_map);
    let resolved_template = resolve_slots(template_nodes, &slots);

    // Attribute forwarding?
    // forwardAttributesToRoot logic...
    // Let's implement simpler forwarding logic or skip for now.
    // Ideally we should forward `class`, `style` etc.
    // For now, let's recurse.

    resolve_nodes(resolved_template, ctx, depth + 1)
}

fn rewrite_node_expressions(
    nodes: &mut Vec<TemplateNode>,
    id_map: &HashMap<String, String>,
    rename_map: &HashMap<String, String>,
) {
    for node in nodes {
        match node {
            TemplateNode::Expression(e) => {
                if let Some(new_id) = id_map.get(&e.expression) {
                    e.expression = new_id.clone();
                }
                // Note: ExpressionNode only has `expression` (ID string), not the raw code
                // The actual code lives in ExpressionIR in the expressions array
            }
            TemplateNode::Element(elem) => {
                for attr in &mut elem.attributes {
                    // Event handlers are transformed to data-zen-* format by parseTemplate
                    // Check for: data-zen-click, data-zen-change, data-zen-input, data-zen-submit
                    let is_event_handler = attr.name == "data-zen-click"
                        || attr.name == "data-zen-change"
                        || attr.name == "data-zen-input"
                        || attr.name == "data-zen-submit"
                        || attr.name.starts_with("on"); // fallback for untransformed

                    match &mut attr.value {
                        crate::validate::AttributeValue::Dynamic(expr) => {
                            if let Some(new_id) = id_map.get(&expr.id) {
                                expr.id = new_id.clone();
                            }
                            expr.code = rename_symbols_safe(&expr.code, rename_map);
                        }
                        crate::validate::AttributeValue::Static(value) => {
                            // For event handlers, rename the function reference
                            if is_event_handler {
                                if let Some(new_name) = rename_map.get(value.as_str()) {
                                    *value = new_name.clone();
                                }
                            }
                        }
                    }
                }
                rewrite_node_expressions(&mut elem.children, id_map, rename_map);
            }
            TemplateNode::Component(comp) => {
                // Also rewrite component attributes
                for attr in &mut comp.attributes {
                    let is_event_handler = attr.name.starts_with("on");
                    match &mut attr.value {
                        crate::validate::AttributeValue::Dynamic(expr) => {
                            if let Some(new_id) = id_map.get(&expr.id) {
                                expr.id = new_id.clone();
                            }
                            expr.code = rename_symbols_safe(&expr.code, rename_map);
                        }
                        crate::validate::AttributeValue::Static(value) => {
                            if is_event_handler {
                                if let Some(new_name) = rename_map.get(value.as_str()) {
                                    *value = new_name.clone();
                                }
                            }
                        }
                    }
                }
                rewrite_node_expressions(&mut comp.children, id_map, rename_map);
            }
            TemplateNode::ConditionalFragment(cf) => {
                if let Some(new_id) = id_map.get(&cf.condition) {
                    cf.condition = new_id.clone();
                }
                rewrite_node_expressions(&mut cf.consequent, id_map, rename_map);
                rewrite_node_expressions(&mut cf.alternate, id_map, rename_map);
            }
            TemplateNode::LoopFragment(lf) => {
                if let Some(new_id) = id_map.get(&lf.source) {
                    lf.source = new_id.clone();
                }
                rewrite_node_expressions(&mut lf.body, id_map, rename_map);
            }
            TemplateNode::OptionalFragment(of) => {
                if let Some(new_id) = id_map.get(&of.condition) {
                    of.condition = new_id.clone();
                }
                rewrite_node_expressions(&mut of.fragment, id_map, rename_map);
            }
            _ => {}
        }
    }
}

fn extract_slots(
    parent_name: &str,
    children: Vec<TemplateNode>,
    parent_scope: Option<LoopContext>,
) -> ResolvedSlots {
    let mut default = Vec::new();
    let mut named = HashMap::new();

    for child in children {
        let mut is_named = false;

        if let TemplateNode::Component(ref comp) = child {
            if let Some(slot_name) = parse_compound_name(&comp.name, parent_name) {
                is_named = true;
                let scoped_children = comp
                    .children
                    .iter()
                    .map(|c| rebind_node_to_scope(c.clone(), &parent_scope))
                    .collect::<Vec<_>>();

                named
                    .entry(slot_name)
                    .or_insert_with(Vec::new)
                    .extend(scoped_children);
            }
        }

        if !is_named {
            default.push(rebind_node_to_scope(child, &parent_scope));
        }
    }

    ResolvedSlots {
        default,
        named,
        parent_loop_context: parent_scope,
    }
}

fn parse_compound_name(component_name: &str, parent_name: &str) -> Option<String> {
    let prefix = format!("{}.", parent_name);
    if component_name.starts_with(&prefix) {
        Some(component_name[prefix.len()..].to_lowercase())
    } else {
        None
    }
}

fn rebind_node_to_scope(node: TemplateNode, loop_context: &Option<LoopContext>) -> TemplateNode {
    if loop_context.is_none() {
        return node;
    }
    let lc = loop_context.as_ref().unwrap();

    match node {
        TemplateNode::Element(mut elem) => {
            elem.loop_context = merge_loop_context(&elem.loop_context, loop_context);
            // Attributes rebinding if dynamic?
            // TS impl: node.attributes.map(attr => ... merge ...)
            for attr in &mut elem.attributes {
                attr.loop_context = merge_loop_context(&attr.loop_context, loop_context);
            }
            elem.children = elem
                .children
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            TemplateNode::Element(elem)
        }
        TemplateNode::Component(mut comp) => {
            comp.loop_context = merge_loop_context(&comp.loop_context, loop_context);
            comp.children = comp
                .children
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            TemplateNode::Component(comp)
        }
        TemplateNode::Expression(mut expr) => {
            expr.loop_context = merge_loop_context(&expr.loop_context, loop_context);
            TemplateNode::Expression(expr)
        }
        TemplateNode::ConditionalFragment(mut cf) => {
            cf.loop_context = merge_loop_context(&cf.loop_context, loop_context);
            cf.consequent = cf
                .consequent
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            cf.alternate = cf
                .alternate
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            TemplateNode::ConditionalFragment(cf)
        }
        TemplateNode::OptionalFragment(mut of) => {
            of.loop_context = merge_loop_context(&of.loop_context, loop_context);
            of.fragment = of
                .fragment
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            TemplateNode::OptionalFragment(of)
        }
        TemplateNode::LoopFragment(mut lf) => {
            lf.loop_context = merge_loop_context(&lf.loop_context, loop_context);
            // Loop Fragment body already has its own scope derived from source,
            // but if parent scope has vars, they should flow through?
            // TS impl doesn't explicitly recurse body for loop fragment because the body scope is generated later?
            // Actually TS `rebindNodeToScope` handles default case as return node.
            // For LoopFragment, we usually don't rebind because it creates a NEW scope boundary.
            // But variables from parent scope *should* be available.
            // mergeLoopContext handles merging variables.
            // So we should merge, but maybe not recurse if variables are shadowed?
            // Let's recurse to be safe.
            lf.body = lf
                .body
                .into_iter()
                .map(|c| rebind_node_to_scope(c, loop_context))
                .collect();
            TemplateNode::LoopFragment(lf)
        }
        _ => node,
    }
}

fn merge_loop_context(
    existing: &Option<LoopContext>,
    parent: &Option<LoopContext>,
) -> Option<LoopContext> {
    if existing.is_none() && parent.is_none() {
        return None;
    }
    if existing.is_none() {
        return parent.clone();
    }
    if parent.is_none() {
        return existing.clone();
    }

    let ex = existing.as_ref().unwrap();
    let p = parent.as_ref().unwrap();

    let mut vars = ex.variables.clone();
    for v in &p.variables {
        if !vars.contains(v) {
            vars.push(v.clone());
        }
    }

    Some(LoopContext {
        variables: vars,
        map_source: p.map_source.clone().or(ex.map_source.clone()),
    })
}

fn resolve_slots(nodes: Vec<TemplateNode>, slots: &ResolvedSlots) -> Vec<TemplateNode> {
    let mut resolved = Vec::new();
    for node in nodes {
        match node {
            TemplateNode::Element(ref elem) if elem.tag == "slot" => {
                // Find name attr
                let name = elem
                    .attributes
                    .iter()
                    .find(|a| a.name == "name")
                    .and_then(|a| match &a.value {
                        crate::validate::AttributeValue::Static(s) => Some(s.clone()),
                        _ => None,
                    });

                if let Some(n) = name {
                    if let Some(content) = slots.named.get(&n) {
                        resolved.extend(content.clone());
                        continue;
                    }
                } else {
                    // Default slot
                    if !slots.default.is_empty() {
                        resolved.extend(slots.default.clone());
                        continue;
                    }
                }

                // Fallback content
                resolved.extend(resolve_slots(elem.children.clone(), slots));
            }
            TemplateNode::Element(mut elem) => {
                elem.children = resolve_slots(elem.children, slots);
                resolved.push(TemplateNode::Element(elem));
            }
            // Recurse other types...
            _ => resolved.push(node),
        }
    }
    resolved
}

/// Robust symbol renaming using Oxc parser.
/// Renames identifiers in `code` based on `rename_map`.
/// Avoids renaming object properties (e.g. `obj.prop`).
pub fn rename_symbols_safe(code: &str, rename_map: &HashMap<String, String>) -> String {
    if rename_map.is_empty() {
        return code.to_string();
    }

    // Preprocess: Replace "state " with "let " so Oxc can parse Zenith's custom keyword
    let parsable_code = code.replace("state ", "let ");
    let used_state_preprocessing = parsable_code != code;

    let allocator = Allocator::default();
    let source_type = SourceType::default()
        .with_module(true)
        .with_typescript(true)
        .with_jsx(true);
    let ret = Parser::new(&allocator, &parsable_code, source_type).parse();
    if !ret.errors.is_empty() {
        return code.to_string();
    }

    let program = ret.program;

    // Collect (start, end, new_name) tuples for replacements
    let mut replacements: Vec<(u32, u32, String)> = Vec::new();

    for stmt in program.body {
        collect_replacements_stmt(&stmt, rename_map, &mut replacements);
    }

    // Sort reverse to apply safely
    replacements.sort_by(|a, b| b.0.cmp(&a.0));

    let mut result = parsable_code.to_string();
    for (start, end, replacement) in replacements {
        result.replace_range((start as usize)..(end as usize), &replacement);
    }

    // Restore "state " keyword if we preprocessed it
    if used_state_preprocessing {
        result = result.replace("let ", "state ");
    }

    result
}

fn get_local_declarations(script: &str) -> HashSet<String> {
    // Preprocess: Replace "state " with "let " so Oxc can parse Zenith's custom keyword
    let parsable_script = script.replace("state ", "let ");

    let allocator = Allocator::default();
    let source_type = SourceType::default()
        .with_module(true)
        .with_typescript(true)
        .with_jsx(true);
    let ret = Parser::new(&allocator, &parsable_script, source_type).parse();

    let mut symbols = HashSet::new();
    if !ret.errors.is_empty() {
        return symbols;
    }

    for stmt in ret.program.body {
        match stmt {
            Statement::VariableDeclaration(var_decl) => {
                for decl in &var_decl.declarations {
                    collect_binding_pattern(&decl.id, &mut symbols);
                }
            }
            Statement::FunctionDeclaration(func_decl) => {
                if let Some(id) = &func_decl.id {
                    symbols.insert(id.name.to_string());
                }
            }
            Statement::ClassDeclaration(class_decl) => {
                if let Some(id) = &class_decl.id {
                    symbols.insert(id.name.to_string());
                }
            }
            _ => {}
        }
    }

    symbols
}

fn collect_binding_pattern(pattern: &oxc_ast::ast::BindingPattern, symbols: &mut HashSet<String>) {
    match pattern {
        oxc_ast::ast::BindingPattern::BindingIdentifier(id) => {
            symbols.insert(id.name.to_string());
        }
        oxc_ast::ast::BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                collect_binding_pattern(&prop.value, symbols);
            }
            if let Some(rest) = &obj.rest {
                collect_binding_pattern(&rest.argument, symbols);
            }
        }
        oxc_ast::ast::BindingPattern::ArrayPattern(arr) => {
            for elem in &arr.elements {
                if let Some(pattern) = elem {
                    collect_binding_pattern(pattern, symbols);
                }
            }
            if let Some(rest) = &arr.rest {
                collect_binding_pattern(&rest.argument, symbols);
            }
        }
        _ => {}
    }
}

fn collect_replacements_stmt(
    stmt: &Statement,
    map: &HashMap<String, String>,
    replacements: &mut Vec<(u32, u32, String)>,
) {
    match stmt {
        Statement::VariableDeclaration(var) => {
            for decl in &var.declarations {
                collect_replacements_binding(&decl.id, map, replacements);
                if let Some(init) = &decl.init {
                    collect_replacements_expr(init, map, replacements);
                }
            }
        }
        Statement::FunctionDeclaration(func) => {
            if let Some(id) = &func.id {
                if let Some(new_name) = map.get(&id.name.to_string()) {
                    replacements.push((id.span.start, id.span.end, new_name.clone()));
                }
            }
            if let Some(body) = &func.body {
                for s in &body.statements {
                    collect_replacements_stmt(s, map, replacements);
                }
            }
            for param in &func.params.items {
                collect_replacements_binding(&param.pattern, map, replacements);
            }
        }
        Statement::ClassDeclaration(cls) => {
            if let Some(id) = &cls.id {
                if let Some(new_name) = map.get(&id.name.to_string()) {
                    replacements.push((id.span.start, id.span.end, new_name.clone()));
                }
            }
        }
        Statement::ExpressionStatement(expr_stmt) => {
            collect_replacements_expr(&expr_stmt.expression, map, replacements);
        }
        Statement::BlockStatement(blk) => {
            for s in &blk.body {
                collect_replacements_stmt(s, map, replacements);
            }
        }
        Statement::IfStatement(if_stmt) => {
            collect_replacements_expr(&if_stmt.test, map, replacements);
            collect_replacements_stmt(&if_stmt.consequent, map, replacements);
            if let Some(alt) = &if_stmt.alternate {
                collect_replacements_stmt(alt, map, replacements);
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                collect_replacements_expr(arg, map, replacements);
            }
        }
        _ => {}
    }
}

fn collect_replacements_expr(
    expr: &Expression,
    map: &HashMap<String, String>,
    replacements: &mut Vec<(u32, u32, String)>,
) {
    match expr {
        Expression::Identifier(id) => {
            if let Some(new_name) = map.get(&id.name.to_string()) {
                replacements.push((id.span.start, id.span.end, new_name.clone()));
            }
        }
        Expression::BinaryExpression(bin) => {
            collect_replacements_expr(&bin.left, map, replacements);
            collect_replacements_expr(&bin.right, map, replacements);
        }
        // UpdateExpression: count++, --count, etc.
        Expression::UpdateExpression(update) => {
            // argument is SimpleAssignmentTarget, not Expression
            match &update.argument {
                oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                    if let Some(new_name) = map.get(&id.name.to_string()) {
                        replacements.push((id.span.start, id.span.end, new_name.clone()));
                    }
                }
                oxc_ast::ast::SimpleAssignmentTarget::StaticMemberExpression(st) => {
                    collect_replacements_expr(&st.object, map, replacements);
                }
                oxc_ast::ast::SimpleAssignmentTarget::ComputedMemberExpression(comp) => {
                    collect_replacements_expr(&comp.object, map, replacements);
                    collect_replacements_expr(&comp.expression, map, replacements);
                }
                _ => {}
            }
        }
        // AssignmentExpression: count = 5, count += 1, etc.
        Expression::AssignmentExpression(assign) => {
            // Left side can be SimpleAssignmentTarget (Identifier) or AssignmentTargetPattern
            match &assign.left {
                oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) => {
                    if let Some(new_name) = map.get(&id.name.to_string()) {
                        replacements.push((id.span.start, id.span.end, new_name.clone()));
                    }
                }
                oxc_ast::ast::AssignmentTarget::StaticMemberExpression(st) => {
                    collect_replacements_expr(&st.object, map, replacements);
                }
                oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(comp) => {
                    collect_replacements_expr(&comp.object, map, replacements);
                    collect_replacements_expr(&comp.expression, map, replacements);
                }
                _ => {}
            }
            collect_replacements_expr(&assign.right, map, replacements);
        }
        // UnaryExpression: !flag, -num, typeof x, etc.
        Expression::UnaryExpression(unary) => {
            collect_replacements_expr(&unary.argument, map, replacements);
        }
        // LogicalExpression: a && b, a || b, a ?? b
        Expression::LogicalExpression(logical) => {
            collect_replacements_expr(&logical.left, map, replacements);
            collect_replacements_expr(&logical.right, map, replacements);
        }
        // ConditionalExpression: a ? b : c
        Expression::ConditionalExpression(cond) => {
            collect_replacements_expr(&cond.test, map, replacements);
            collect_replacements_expr(&cond.consequent, map, replacements);
            collect_replacements_expr(&cond.alternate, map, replacements);
        }
        // ParenthesizedExpression: (expr)
        Expression::ParenthesizedExpression(paren) => {
            collect_replacements_expr(&paren.expression, map, replacements);
        }
        // SequenceExpression: a, b, c
        Expression::SequenceExpression(seq) => {
            for e in &seq.expressions {
                collect_replacements_expr(e, map, replacements);
            }
        }
        // TemplateLiteral: `hello ${name}`
        Expression::TemplateLiteral(tpl) => {
            for expr in &tpl.expressions {
                collect_replacements_expr(expr, map, replacements);
            }
        }
        // AwaitExpression: await promise
        Expression::AwaitExpression(await_expr) => {
            collect_replacements_expr(&await_expr.argument, map, replacements);
        }
        // YieldExpression: yield value
        Expression::YieldExpression(yield_expr) => {
            if let Some(arg) = &yield_expr.argument {
                collect_replacements_expr(arg, map, replacements);
            }
        }
        Expression::CallExpression(call) => {
            collect_replacements_expr(&call.callee, map, replacements);
            for arg in &call.arguments {
                if let Some(e) = arg.as_expression() {
                    collect_replacements_expr(e, map, replacements);
                }
            }
        }
        Expression::ComputedMemberExpression(comp) => {
            collect_replacements_expr(&comp.object, map, replacements);
            collect_replacements_expr(&comp.expression, map, replacements);
        }
        Expression::StaticMemberExpression(st) => {
            if let Expression::Identifier(obj_id) = &st.object {
                if obj_id.name == "props" {
                    let prop_name = st.property.name.to_string();
                    let full_name = format!("props.{}", prop_name);
                    if let Some(new_name) = map.get(&full_name) {
                        replacements.push((st.span.start, st.span.end, new_name.clone()));
                        return;
                    }
                }
            }
            collect_replacements_expr(&st.object, map, replacements);
        }
        Expression::PrivateFieldExpression(p) => {
            collect_replacements_expr(&p.object, map, replacements);
        }
        Expression::ObjectExpression(obj) => {
            for prop in &obj.properties {
                match prop {
                    oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) => {
                        if p.shorthand {
                            if let PropertyKey::StaticIdentifier(id) = &p.key {
                                if let Some(new_name) = map.get(&id.name.to_string()) {
                                    let replacement = format!("{}: {}", id.name, new_name);
                                    replacements.push((p.span.start, p.span.end, replacement));
                                }
                            }
                        } else {
                            collect_replacements_expr(&p.value, map, replacements);
                            if p.computed {
                                if let Some(e) = p.key.as_expression() {
                                    collect_replacements_expr(e, map, replacements);
                                }
                            }
                        }
                    }
                    oxc_ast::ast::ObjectPropertyKind::SpreadProperty(s) => {
                        collect_replacements_expr(&s.argument, map, replacements);
                    }
                }
            }
        }
        Expression::ArrayExpression(arr) => {
            for elem in &arr.elements {
                if let Some(e) = elem.as_expression() {
                    collect_replacements_expr(e, map, replacements);
                }
            }
        }
        Expression::ArrowFunctionExpression(func) => {
            for param in &func.params.items {
                collect_replacements_binding(&param.pattern, map, replacements);
            }
            for s in &func.body.statements {
                collect_replacements_stmt(s, map, replacements);
            }
        }
        Expression::NewExpression(new_expr) => {
            collect_replacements_expr(&new_expr.callee, map, replacements);
            for arg in &new_expr.arguments {
                if let Some(e) = arg.as_expression() {
                    collect_replacements_expr(e, map, replacements);
                }
            }
        }
        _ => {}
    }
}

fn collect_replacements_binding(
    pattern: &oxc_ast::ast::BindingPattern,
    map: &HashMap<String, String>,
    replacements: &mut Vec<(u32, u32, String)>,
) {
    match pattern {
        oxc_ast::ast::BindingPattern::BindingIdentifier(id) => {
            if let Some(new_name) = map.get(&id.name.to_string()) {
                replacements.push((id.span.start, id.span.end, new_name.clone()));
            }
        }
        oxc_ast::ast::BindingPattern::ObjectPattern(obj) => {
            for prop in &obj.properties {
                collect_replacements_binding(&prop.value, map, replacements);
            }
            if let Some(rest) = &obj.rest {
                collect_replacements_binding(&rest.argument, map, replacements);
            }
        }
        oxc_ast::ast::BindingPattern::ArrayPattern(arr) => {
            for elem in &arr.elements {
                if let Some(pattern) = elem {
                    collect_replacements_binding(pattern, map, replacements);
                }
            }
            if let Some(rest) = &arr.rest {
                collect_replacements_binding(&rest.argument, map, replacements);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::{ElementNode, ExpressionIR, SourceLocation, TemplateNode};
    use std::collections::HashMap;

    fn mock_loc() -> SourceLocation {
        SourceLocation { line: 1, column: 1 }
    }

    #[test]
    fn test_extract_slots_default() {
        let children = vec![TemplateNode::Element(ElementNode {
            tag: "div".to_string(),
            attributes: vec![],
            children: vec![],
            location: mock_loc(),
            loop_context: None,
        })];

        let slots = extract_slots("Card", children, None);
        assert_eq!(slots.default.len(), 1);
        assert!(slots.named.is_empty());
    }

    #[test]
    fn test_extract_slots_named() {
        let header_node = TemplateNode::Component(crate::validate::ComponentNode {
            name: "Card.Header".to_string(),
            attributes: vec![],
            children: vec![TemplateNode::Element(ElementNode {
                tag: "h1".to_string(),
                attributes: vec![],
                children: vec![],
                location: mock_loc(),
                loop_context: None,
            })],
            location: mock_loc(),
            loop_context: None,
        });

        let children = vec![header_node];

        let slots = extract_slots("Card", children, None);
        assert!(slots.default.is_empty());
        assert_eq!(slots.named.get("header").unwrap().len(), 1);
    }

    #[test]
    fn test_rename_symbols_simple() {
        let code = "const a = 1; let b = 2; console.log(a, b);";
        let mut map = HashMap::new();
        map.insert("a".to_string(), "a_1".to_string());
        map.insert("b".to_string(), "b_1".to_string());

        let renamed = rename_symbols_safe(code, &map);
        assert!(renamed.contains("const a_1 = 1"));
        assert!(renamed.contains("let b_1 = 2"));
        assert!(renamed.contains("console.log(a_1, b_1)"));
    }

    #[test]
    fn test_rename_symbols_object_property() {
        let code = "const a = 1; const obj = { a: a, b: 2 };";
        let mut map = HashMap::new();
        map.insert("a".to_string(), "a_1".to_string());

        let renamed = rename_symbols_safe(code, &map);

        // Expected: const a_1 = 1; const obj = { a: a_1, b: 2 };
        assert!(renamed.contains("const a_1 = 1"));
        // Check that property key 'a' is preserved but value is renamed
        // Regex to check structure approximately
        assert!(renamed.contains("a: a_1"));
    }

    #[test]
    fn test_rename_symbols_shorthand() {
        let code = "const a = 1; const obj = { a };";
        let mut map = HashMap::new();
        map.insert("a".to_string(), "a_1".to_string());

        let renamed = rename_symbols_safe(code, &map);

        // Expected: const a_1 = 1; const obj = { a: a_1 };
        assert!(renamed.contains("const a_1 = 1"));
        assert!(renamed.contains("a: a_1"));
    }
}
