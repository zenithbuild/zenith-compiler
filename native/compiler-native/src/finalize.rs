//! Finalize Module for Zenith Compiler
//!
//! Port of finalizeOutput.ts to Rust.
//! Generates final HTML+JS and validation.

use napi_derive::napi;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

use crate::codegen::{generate_runtime_code_internal, CodegenInput, ScriptImport};
use crate::validate::{ExpressionInput, LoopContext, LoopContextInput, ZenIR};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledTemplate {
    pub html: String,
    pub styles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct VirtualModule {
    pub id: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct BundlePlan {
    pub entry: String,
    pub platform: String,
    pub format: String,
    pub resolve_roots: Vec<String>,
    pub virtual_modules: Vec<VirtualModule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[napi(object)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedOutput {
    pub html: String,
    pub js: String,
    pub npm_imports: String,
    pub styles: Vec<String>,
    pub has_errors: bool,
    pub errors: Vec<String>,
    pub bundle_plan: Option<BundlePlan>,
}

fn emit_imports(imports: &[ScriptImport]) -> String {
    imports
        .iter()
        .map(|imp| {
            if imp.specifiers.is_empty() {
                format!("import '{}';", imp.source)
            } else {
                // Basic heuristic for default vs named imports
                // If specifiers starts with { or *, assume it's correct syntax
                if imp.specifiers.trim().starts_with('{') || imp.specifiers.trim().starts_with('*')
                {
                    format!("import {} from '{}';", imp.specifiers, imp.source)
                } else {
                    format!("import {} from '{}';", imp.specifiers, imp.source)
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn verify_no_raw_expressions(html: &str, file_path: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let re = Regex::new(r"\{[^}]*\}").unwrap();

    let mut actual_expressions = Vec::new();
    for mat in re.find_iter(html) {
        let s = mat.as_str();

        // Exclusions matching TS logic
        if html.contains(&format!("<!--{}", s)) || html.contains(&format!("{}-->", s)) {
            continue;
        }
        if s.contains("data-zen-") {
            continue;
        }
        if s.starts_with("{<") || s.contains("\n") {
            continue;
        } // Rough heuristic for tags
        if s.contains(";") && s.contains(":") {
            continue;
        } // CSS-like

        // TS regex: /^\{[\s]*</
        if s.starts_with("{") && s[1..].trim_start().starts_with("<") {
            continue;
        }
        // TS regex: /<[a-zA-Z]/ inside
        if s.contains("<") {
            continue;
        }

        actual_expressions.push(s.to_string());
    }

    if !actual_expressions.is_empty() {
        errors.push(format!(
            "HTML contains raw expressions that were not compiled: {}\nFile: {}\nAll expressions must be replaced with hydration markers.",
            actual_expressions.join(", "),
            file_path
        ));
    }

    errors
}

#[napi]
pub fn finalize_output_native(
    ir_json: serde_json::Value,
    compiled_json: serde_json::Value,
) -> napi::Result<FinalizedOutput> {
    let ir: ZenIR = serde_json::from_value(ir_json)
        .map_err(|e| napi::Error::from_reason(format!("Invalid IR: {}", e)))?;

    let compiled: CompiledTemplate = serde_json::from_value(compiled_json)
        .map_err(|e| napi::Error::from_reason(format!("Invalid CompiledTemplate: {}", e)))?;

    // Verify HTML
    let html_errors = verify_no_raw_expressions(&compiled.html, &ir.file_path);
    if !html_errors.is_empty() {
        return Ok(FinalizedOutput {
            has_errors: true,
            errors: html_errors,
            html: String::new(),
            js: String::new(),
            npm_imports: String::new(),
            styles: vec![],
            bundle_plan: None,
        });
    }

    // Prepare Codegen Input
    let script_content = ir
        .script
        .as_ref()
        .map(|s| s.raw.clone())
        .unwrap_or_default();

    // Map expressions
    let expressions: Vec<ExpressionInput> = ir
        .template
        .expressions
        .iter()
        .map(|e| ExpressionInput {
            id: e.id.clone(),
            code: e.code.clone(),
            loop_context: e.loop_context.as_ref().map(|lc| LoopContextInput {
                variables: lc.variables.clone(),
                map_source: lc.map_source.clone(),
            }),
        })
        .collect();

    let codegen_input = CodegenInput {
        file_path: ir.file_path.clone(),
        script_content,
        expressions,
        styles: ir.styles,
        template_bindings: vec![], // TODO: Extract bindings if needed? Codegen re-extracts?
        location: ir.file_path.clone(), // Using file path as location
        nodes: ir.template.nodes,
        page_bindings: vec![], // Can be populated if needed
    };

    let runtime_code = generate_runtime_code_internal(codegen_input);

    let final_imports = emit_imports(&runtime_code.npm_imports);

    // BundlePlan
    let bundle_plan = if !runtime_code.npm_imports.is_empty() {
        let entry_code = runtime_code.bundle.clone();

        let resolve_root = Path::new(&ir.file_path)
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_string_lossy()
            .to_string();

        Some(BundlePlan {
            entry: entry_code,
            platform: "browser".to_string(),
            format: "esm".to_string(),
            resolve_roots: vec![resolve_root],
            virtual_modules: vec![
                VirtualModule {
                    id: "\0zenith:content".to_string(),
                    code: "export const zenCollection = (typeof globalThis !== 'undefined' ? globalThis : window).zenCollection;".to_string(),
                }
            ]
        })
    } else {
        None
    };

    Ok(FinalizedOutput {
        html: compiled.html,
        js: runtime_code.bundle,
        npm_imports: final_imports,
        styles: compiled.styles,
        has_errors: false,
        errors: vec![],
        bundle_plan,
    })
}
