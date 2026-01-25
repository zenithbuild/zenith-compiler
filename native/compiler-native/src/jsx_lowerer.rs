//! JSX/Script Lowering for Zenith Compiler

use oxc_allocator::{Allocator, Box as oxc_box, CloneIn};
use oxc_ast::ast::*;
use oxc_ast::AstBuilder;
use oxc_ast_visit::walk_mut::{
    walk_arrow_function_expression, walk_assignment_target, walk_catch_clause, walk_expression,
    walk_for_in_statement, walk_for_of_statement, walk_for_statement, walk_function, walk_program,
    walk_simple_assignment_target, walk_statement,
};
use oxc_ast_visit::VisitMut;
use oxc_span::SPAN;
use oxc_syntax::scope::ScopeFlags;
use std::collections::HashSet;

// ═══════════════════════════════════════════════════════════════════════════════
// JSX LOWERER
// Transforms JSX elements into __zenith.h() calls
// ═══════════════════════════════════════════════════════════════════════════════

pub struct JsxLowerer<'a> {
    pub ast: AstBuilder<'a>,
}

impl<'a> JsxLowerer<'a> {
    pub fn new(allocator: &'a Allocator) -> Self {
        Self {
            ast: AstBuilder::new(allocator),
        }
    }

    fn lower_jsx_element(&mut self, element: &JSXElement<'a>) -> Expression<'a> {
        let tag_name = self.get_tag_name(&element.opening_element.name);
        let tag_atom = self.ast.allocator.alloc_str(&tag_name);

        let mut current_obj_props = self.ast.vec();

        for item in &element.opening_element.attributes {
            match item {
                JSXAttributeItem::Attribute(attr) => {
                    let name = match &attr.name {
                        JSXAttributeName::Identifier(id) => PropertyKey::StaticIdentifier(
                            self.ast
                                .alloc(self.ast.identifier_name(SPAN, id.name.clone())),
                        ),
                        JSXAttributeName::NamespacedName(ns) => {
                            let ns_name = format!("{}:{}", ns.namespace.name, ns.name.name);
                            let ns_atom = self.ast.allocator.alloc_str(&ns_name);
                            PropertyKey::StaticIdentifier(
                                self.ast.alloc(self.ast.identifier_name(SPAN, ns_atom)),
                            )
                        }
                    };

                    let value = match &attr.value {
                        Some(JSXAttributeValue::StringLiteral(s)) => {
                            Expression::StringLiteral(self.ast.alloc((**s).clone()))
                        }
                        Some(JSXAttributeValue::Element(el)) => self.lower_jsx_element(el),
                        Some(JSXAttributeValue::ExpressionContainer(container)) => {
                            let jsx_expr = &container.expression;
                            // Convert JSXExpression to Expression using as_expression() hint from compiler
                            // If it's not a direct expression, we'll get None (e.g. EmptyExpression)
                            if let Some(mut e) = jsx_expr
                                .as_expression()
                                .map(|e| e.clone_in(self.ast.allocator))
                            {
                                self.visit_expression(&mut e);
                                e
                            } else {
                                self.ast.expression_identifier(SPAN, "undefined")
                            }
                        }
                        Some(JSXAttributeValue::Fragment(frag)) => self.lower_jsx_fragment(frag),
                        None => self.ast.expression_boolean_literal(SPAN, true),
                    };

                    current_obj_props.push(self.ast.object_property_kind_object_property(
                        SPAN,
                        PropertyKind::Init,
                        name,
                        value,
                        false,
                        false,
                        false,
                    ));
                }
                JSXAttributeItem::SpreadAttribute(spread) => {
                    let mut spread_expr = spread.argument.clone_in(self.ast.allocator);
                    self.visit_expression(&mut spread_expr);
                    current_obj_props.push(
                        self.ast
                            .object_property_kind_spread_property(SPAN, spread_expr),
                    );
                }
            }
        }

        let props_expr = if current_obj_props.is_empty() {
            self.ast.expression_identifier(SPAN, "null")
        } else {
            self.ast.expression_object(SPAN, current_obj_props)
        };

        // Children -> Array or Null
        let mut children_vec = self.ast.vec();
        for child in &element.children {
            match child {
                JSXChild::Text(t) => {
                    let text = t.value.trim();
                    if !text.is_empty() {
                        let text_atom = self.ast.allocator.alloc_str(text);
                        children_vec.push(ArrayExpressionElement::from(
                            self.ast.expression_string_literal(SPAN, text_atom, None),
                        ));
                    }
                }
                JSXChild::Element(el) => {
                    children_vec.push(ArrayExpressionElement::from(self.lower_jsx_element(el)));
                }
                JSXChild::Fragment(frag) => {
                    children_vec.push(ArrayExpressionElement::from(self.lower_jsx_fragment(frag)));
                }
                JSXChild::ExpressionContainer(container) => {
                    children_vec.push(ArrayExpressionElement::from(
                        self.lower_jsx_expression(&container.expression),
                    ));
                }
                JSXChild::Spread(spread) => {
                    let mut arg = spread.expression.clone_in(self.ast.allocator);
                    self.visit_expression(&mut arg);
                    children_vec.push(ArrayExpressionElement::from(arg));
                }
            }
        }

        let children_expr = if children_vec.is_empty() {
            self.ast.expression_identifier(SPAN, "null")
        } else {
            self.ast.expression_array(SPAN, children_vec)
        };

        let mut args = self.ast.vec();
        args.push(Argument::from(
            self.ast.expression_string_literal(SPAN, tag_atom, None),
        ));
        args.push(Argument::from(props_expr));
        args.push(Argument::from(children_expr));

        let callee = Expression::from(
            self.ast.member_expression_static(
                SPAN,
                self.ast
                    .member_expression_static(
                        SPAN,
                        self.ast.expression_identifier(SPAN, "window"),
                        self.ast.identifier_name(SPAN, "__zenith"),
                        false,
                    )
                    .into(),
                self.ast.identifier_name(SPAN, "h"),
                false,
            ),
        );

        self.ast.expression_call(
            SPAN,
            callee,
            None::<oxc_box<TSTypeParameterInstantiation>>,
            args,
            false,
        )
    }

    fn lower_jsx_fragment(&mut self, fragment: &JSXFragment<'a>) -> Expression<'a> {
        let mut children_vec = self.ast.vec();
        for child in &fragment.children {
            match child {
                JSXChild::Text(t) => {
                    let text = t.value.trim();
                    if !text.is_empty() {
                        let text_atom = self.ast.allocator.alloc_str(text);
                        children_vec.push(ArrayExpressionElement::from(
                            self.ast.expression_string_literal(SPAN, text_atom, None),
                        ));
                    }
                }
                JSXChild::Element(el) => {
                    children_vec.push(ArrayExpressionElement::from(self.lower_jsx_element(el)));
                }
                JSXChild::Fragment(frag) => {
                    children_vec.push(ArrayExpressionElement::from(self.lower_jsx_fragment(frag)));
                }
                JSXChild::ExpressionContainer(container) => {
                    children_vec.push(ArrayExpressionElement::from(
                        self.lower_jsx_expression(&container.expression),
                    ));
                }
                JSXChild::Spread(spread) => {
                    let mut arg = spread.expression.clone_in(self.ast.allocator);
                    self.visit_expression(&mut arg);
                    children_vec.push(ArrayExpressionElement::from(arg));
                }
            }
        }

        let children_expr = if children_vec.is_empty() {
            self.ast.expression_identifier(SPAN, "null")
        } else {
            self.ast.expression_array(SPAN, children_vec)
        };

        let mut args = self.ast.vec();
        args.push(Argument::from(children_expr));

        let callee = Expression::from(
            self.ast.member_expression_static(
                SPAN,
                self.ast
                    .member_expression_static(
                        SPAN,
                        self.ast.expression_identifier(SPAN, "window"),
                        self.ast.identifier_name(SPAN, "__zenith"),
                        false,
                    )
                    .into(),
                self.ast.identifier_name(SPAN, "fragment"),
                false,
            ),
        );

        self.ast.expression_call(
            SPAN,
            callee,
            None::<oxc_box<TSTypeParameterInstantiation>>,
            args,
            false,
        )
    }

    fn get_tag_name(&self, name: &JSXElementName<'a>) -> String {
        match name {
            JSXElementName::Identifier(id) => id.name.to_string(),
            JSXElementName::IdentifierReference(id) => id.name.to_string(),
            JSXElementName::NamespacedName(ns) => format!("{}:{}", ns.namespace.name, ns.name.name),
            JSXElementName::MemberExpression(me) => self.get_member_name(me),
            JSXElementName::ThisExpression(_) => "this".to_string(),
        }
    }

    fn get_member_name(&self, me: &JSXMemberExpression<'a>) -> String {
        let object = match &me.object {
            JSXMemberExpressionObject::IdentifierReference(id) => id.name.to_string(),
            JSXMemberExpressionObject::MemberExpression(inner) => self.get_member_name(inner),
            _ => "unknown".to_string(),
        };
        format!("{}.{}", object, me.property.name)
    }

    fn lower_jsx_expression(&mut self, jsx_expr: &JSXExpression<'a>) -> Expression<'a> {
        if let Some(mut e) = jsx_expr
            .as_expression()
            .map(|e| e.clone_in(self.ast.allocator))
        {
            self.visit_expression(&mut e);
            e
        } else {
            self.ast.expression_identifier(SPAN, "undefined")
        }
    }
}

impl<'a> VisitMut<'a> for JsxLowerer<'a> {
    fn visit_expression(&mut self, expr: &mut Expression<'a>) {
        match expr {
            Expression::JSXElement(element) => {
                let lowered = self.lower_jsx_element(element);
                *expr = lowered;
            }
            Expression::JSXFragment(fragment) => {
                let lowered = self.lower_jsx_fragment(fragment);
                *expr = lowered;
            }
            Expression::ConditionalExpression(cond) => {
                self.visit_expression(&mut cond.test);
                self.visit_expression(&mut cond.consequent);
                self.visit_expression(&mut cond.alternate);
            }
            Expression::LogicalExpression(logical) => {
                self.visit_expression(&mut logical.left);
                self.visit_expression(&mut logical.right);
            }
            _ => {
                walk_expression(self, expr);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// SCRIPT RENAMER
// Transforms state identifiers to state.prop access pattern
// ═══════════════════════════════════════════════════════════════════════════════

pub struct ScriptRenamer<'a> {
    pub allocator: &'a Allocator,
    pub ast: AstBuilder<'a>,
    pub state_bindings: HashSet<String>,
    pub scope_stack: Vec<HashSet<String>>,
}

impl<'a> ScriptRenamer<'a> {
    pub fn new(allocator: &'a Allocator, state_bindings: HashSet<String>) -> Self {
        Self {
            allocator,
            ast: AstBuilder::new(allocator),
            state_bindings,
            scope_stack: vec![HashSet::new()],
        }
    }

    pub fn with_locals(
        allocator: &'a Allocator,
        state_bindings: HashSet<String>,
        local_vars: HashSet<String>,
    ) -> Self {
        Self {
            allocator,
            ast: AstBuilder::new(allocator),
            state_bindings,
            scope_stack: vec![local_vars],
        }
    }

    pub fn add_local(&mut self, name: String) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(name);
        }
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn is_local(&self, name: &str) -> bool {
        self.scope_stack.iter().rev().any(|s| s.contains(name))
    }

    fn should_rename(&self, name: &str) -> bool {
        self.state_bindings.contains(name) && !self.is_local(name)
    }

    fn create_state_member(&self, prop_name: &str) -> MemberExpression<'a> {
        let arena_str: &'a str = self.allocator.alloc_str(prop_name);
        self.ast.member_expression_static(
            SPAN,
            self.ast.expression_identifier(SPAN, "state"),
            self.ast.identifier_name(SPAN, arena_str),
            false,
        )
    }

    fn is_ts_node(stmt: &Statement<'a>) -> bool {
        match stmt {
            Statement::TSTypeAliasDeclaration(_)
            | Statement::TSInterfaceDeclaration(_)
            | Statement::TSEnumDeclaration(_)
            | Statement::TSModuleDeclaration(_)
            | Statement::TSImportEqualsDeclaration(_) => true,
            _ => false,
        }
    }

    fn collect_binding_names(&mut self, pattern: &BindingPattern<'a>) {
        match pattern {
            BindingPattern::BindingIdentifier(id) => {
                self.add_local(id.name.to_string());
            }
            BindingPattern::ObjectPattern(obj) => {
                for prop in &obj.properties {
                    self.collect_binding_names(&prop.value);
                }
                if let Some(rest) = &obj.rest {
                    self.collect_binding_names(&rest.argument);
                }
            }
            BindingPattern::ArrayPattern(arr) => {
                for elem in &arr.elements {
                    if let Some(p) = elem {
                        self.collect_binding_names(p);
                    }
                }
                if let Some(rest) = &arr.rest {
                    self.collect_binding_names(&rest.argument);
                }
            }
            _ => {}
        }
    }
}

impl<'a> VisitMut<'a> for ScriptRenamer<'a> {
    fn visit_program(&mut self, program: &mut Program<'a>) {
        program.body.retain(|stmt| !Self::is_ts_node(stmt));
        walk_program(self, program);
    }

    fn visit_statement(&mut self, stmt: &mut Statement<'a>) {
        match stmt {
            Statement::BlockStatement(block) => {
                block.body.retain(|s| !Self::is_ts_node(s));
                self.push_scope();
                for s in &mut block.body {
                    self.visit_statement(s);
                }
                self.pop_scope();
            }
            Statement::VariableDeclaration(var_decl) => {
                // If this is a state variable declaration (originally 'state count = ...')
                // we want to transform it into a simple assignment 'state.count = ...'
                // so that it doesn't create a local shadowing variable.
                let mut assignments = self.ast.vec();
                let mut has_state = false;

                for decl in &mut var_decl.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &decl.id {
                        let name = id.name.to_string();
                        if self.state_bindings.contains(&name) {
                            has_state = true;
                            if let Some(init) = &mut decl.init {
                                self.visit_expression(init);
                                let left =
                                    SimpleAssignmentTarget::from(self.create_state_member(&name));
                                assignments.push(self.ast.expression_assignment(
                                    SPAN,
                                    AssignmentOperator::Assign,
                                    AssignmentTarget::from(left),
                                    init.clone_in(self.allocator),
                                ));
                            }
                        } else {
                            self.add_local(name);
                            if let Some(init) = &mut decl.init {
                                self.visit_expression(init);
                            }
                        }
                    } else {
                        // Handle patterns (destructuring)
                        self.collect_binding_names(&decl.id);
                        if let Some(init) = &mut decl.init {
                            self.visit_expression(init);
                        }
                    }
                }

                if has_state && assignments.len() == var_decl.declarations.len() {
                    // All were state variables, turn entire thing into an expression statement (or series)
                    if assignments.len() == 1 {
                        *stmt = self
                            .ast
                            .statement_expression(SPAN, assignments.pop().unwrap());
                    } else {
                        *stmt = self.ast.statement_expression(
                            SPAN,
                            self.ast.expression_sequence(SPAN, assignments),
                        );
                    }
                } else {
                    // Mixed or non-state, just walk (locals already added)
                }
            }
            Statement::FunctionDeclaration(func) => {
                self.push_scope();
                for param in &func.params.items {
                    self.collect_binding_names(&param.pattern);
                }
                if let Some(body) = &mut func.body {
                    for s in &mut body.statements {
                        self.visit_statement(s);
                    }
                }
                self.pop_scope();
            }
            _ => walk_statement(self, stmt),
        }
    }

    fn visit_expression(&mut self, expr: &mut Expression<'a>) {
        // Strip TS specific expressions
        if let Expression::TSAsExpression(as_expr) = expr {
            let inner = as_expr.expression.clone_in(self.allocator);
            *expr = inner;
            self.visit_expression(expr);
            return;
        }
        if let Expression::TSNonNullExpression(nn_expr) = expr {
            let inner = nn_expr.expression.clone_in(self.allocator);
            *expr = inner;
            self.visit_expression(expr);
            return;
        }
        if let Expression::TSSatisfiesExpression(sat_expr) = expr {
            let inner = sat_expr.expression.clone_in(self.allocator);
            *expr = inner;
            self.visit_expression(expr);
            return;
        }

        if let Expression::Identifier(id) = expr {
            let name = id.name.to_string();
            if self.should_rename(&name) {
                println!(
                    "[ZenithNative] Renaming identifier: {} -> state.{}",
                    name, name
                );
                let member = self.create_state_member(&name);
                *expr = Expression::from(member);
                return;
            } else {
                if self.state_bindings.contains(&name) {
                    println!("[ZenithNative] NOT renaming shadowed identifier: {}", name);
                }
            }
        }

        if let Expression::ArrowFunctionExpression(arrow) = expr {
            self.push_scope();
            for param in &arrow.params.items {
                self.collect_binding_names(&param.pattern);
            }
            for stmt in &mut arrow.body.statements {
                self.visit_statement(stmt);
            }
            self.pop_scope();
            return;
        }

        if let Expression::FunctionExpression(func) = expr {
            self.push_scope();
            for param in &func.params.items {
                self.collect_binding_names(&param.pattern);
            }
            if let Some(body) = &mut func.body {
                for s in &mut body.statements {
                    self.visit_statement(s);
                }
            }
            self.pop_scope();
            return;
        }

        walk_expression(self, expr);
    }

    fn visit_assignment_target(&mut self, target: &mut AssignmentTarget<'a>) {
        walk_assignment_target(self, target);
    }

    fn visit_simple_assignment_target(&mut self, target: &mut SimpleAssignmentTarget<'a>) {
        if let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = target {
            let name = id.name.to_string();
            if self.should_rename(&name) {
                println!(
                    "[ZenithNative] Renaming assignment target: {} -> state.{}",
                    name, name
                );
                let member = self.create_state_member(&name);
                *target = SimpleAssignmentTarget::from(member);
                return;
            }
        }
        walk_simple_assignment_target(self, target);
    }

    fn visit_for_of_statement(&mut self, stmt: &mut ForOfStatement<'a>) {
        self.push_scope();
        if let ForStatementLeft::VariableDeclaration(var_decl) = &stmt.left {
            for decl in &var_decl.declarations {
                self.collect_binding_names(&decl.id);
            }
        }
        self.visit_expression(&mut stmt.right);
        self.visit_statement(&mut stmt.body);
        self.pop_scope();
    }

    fn visit_for_in_statement(&mut self, stmt: &mut ForInStatement<'a>) {
        self.push_scope();
        if let ForStatementLeft::VariableDeclaration(var_decl) = &stmt.left {
            for decl in &var_decl.declarations {
                self.collect_binding_names(&decl.id);
            }
        }
        self.visit_expression(&mut stmt.right);
        self.visit_statement(&mut stmt.body);
        self.pop_scope();
    }

    fn visit_for_statement(&mut self, stmt: &mut ForStatement<'a>) {
        self.push_scope();
        if let Some(ForStatementInit::VariableDeclaration(var_decl)) = &stmt.init {
            for decl in &var_decl.declarations {
                self.collect_binding_names(&decl.id);
            }
        }
        if let Some(test) = &mut stmt.test {
            self.visit_expression(test);
        }
        if let Some(update) = &mut stmt.update {
            self.visit_expression(update);
        }
        self.visit_statement(&mut stmt.body);
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, clause: &mut CatchClause<'a>) {
        self.push_scope();
        if let Some(param) = &clause.param {
            self.collect_binding_names(&param.pattern);
        }
        for stmt in &mut clause.body.body {
            self.visit_statement(stmt);
        }
        self.pop_scope();
    }

    fn visit_import_declaration(&mut self, decl: &mut ImportDeclaration<'a>) {
        let source = decl.source.value.to_string();
        if source.ends_with(".zen") {
            let new_source = source.replace(".zen", ".js");
            decl.source.value = self.allocator.alloc_str(&new_source).into();
        }
    }
}
