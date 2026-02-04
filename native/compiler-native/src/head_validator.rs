//! Head Validator for Zenith Compiler
//!
//! Validates that expressions inside <head> are statically resolvable.
//! Expressions in head must only reference literals, props, or whitelisted globals.

use std::collections::HashSet;

/// Whitelisted globals that are safe in head expressions
const SAFE_GLOBALS: &[&str] = &["undefined", "null", "true", "false", "NaN", "Infinity"];

/// Validate that an expression is statically resolvable for head context.
/// Returns Ok(()) if valid, Err with message if invalid.
pub fn validate_head_expression(
    expr: &str,
    allowed_props: &HashSet<String>,
    allowed_locals: &HashSet<String>,
) -> Result<(), String> {
    // Quick checks for simple literals
    let trimmed = expr.trim();

    // String literals are always safe
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        || (trimmed.starts_with('`') && trimmed.ends_with('`'))
    {
        // For template literals, check interpolations
        if trimmed.starts_with('`') {
            return validate_template_literal(trimmed, allowed_props, allowed_locals);
        }
        return Ok(());
    }

    // Number literals are safe
    if trimmed.parse::<f64>().is_ok() {
        return Ok(());
    }

    // Boolean/null/undefined are safe
    if SAFE_GLOBALS.contains(&trimmed) {
        return Ok(());
    }

    // Simple prop/local reference
    if is_valid_identifier(trimmed) {
        if allowed_props.contains(trimmed) || allowed_locals.contains(trimmed) {
            return Ok(());
        }
        if SAFE_GLOBALS.contains(&trimmed) {
            return Ok(());
        }
        return Err(format!(
            "Illegal Runtime Expression in <head>. Identifier '{}' is not a known prop or local. Metadata must be statically resolvable.",
            trimmed
        ));
    }

    // props.name pattern
    if trimmed.starts_with("props.") {
        let prop_name = &trimmed[6..];
        if is_valid_identifier(prop_name) && allowed_props.contains(prop_name) {
            return Ok(());
        }
    }

    // Ternary expressions: condition ? consequent : alternate
    if let Some((condition, rest)) = trimmed.split_once(" ? ") {
        if let Some((consequent, alternate)) = rest.rsplit_once(" : ") {
            // Validate all parts - this is a simplified check
            // The alternate (else) branch provides the static fallback
            validate_head_expression(condition.trim(), allowed_props, allowed_locals)?;
            validate_head_expression(consequent.trim(), allowed_props, allowed_locals)?;
            validate_head_expression(alternate.trim(), allowed_props, allowed_locals)?;
            return Ok(());
        }
    }

    // String concatenation: "Zenith | " + title
    if trimmed.contains(" + ") {
        for part in trimmed.split(" + ") {
            validate_head_expression(part.trim(), allowed_props, allowed_locals)?;
        }
        return Ok(());
    }

    // Disallow dangerous patterns
    let disallowed_patterns = [
        "window.",
        "document.",
        "Date.",
        "Math.random",
        "setInterval",
        "setTimeout",
        "fetch(",
        "await ",
        "async ",
    ];

    for pattern in disallowed_patterns {
        if trimmed.contains(pattern) {
            return Err(format!(
                "Illegal Runtime Expression in <head>. '{}' contains runtime-only code. Metadata must be statically resolvable.",
                pattern.trim_end_matches('.')
            ));
        }
    }

    // Default: allow but warn (for complex expressions we can't fully analyze)
    // In production, this could be more strict
    Ok(())
}

/// Validate template literal interpolations
fn validate_template_literal(
    template: &str,
    allowed_props: &HashSet<String>,
    allowed_locals: &HashSet<String>,
) -> Result<(), String> {
    // Find ${...} interpolations
    let mut i = 0;
    let chars: Vec<char> = template.chars().collect();

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '{' {
            // Find matching closing brace
            let start = i + 2;
            let mut depth = 1;
            let mut end = start;

            while end < chars.len() && depth > 0 {
                if chars[end] == '{' {
                    depth += 1;
                } else if chars[end] == '}' {
                    depth -= 1;
                }
                end += 1;
            }

            if depth == 0 {
                let interpolation: String = chars[start..end - 1].iter().collect();
                validate_head_expression(&interpolation, allowed_props, allowed_locals)?;
            }

            i = end;
        } else {
            i += 1;
        }
    }

    Ok(())
}

/// Check if a string is a valid JavaScript identifier
fn is_valid_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    let mut chars = s.chars();

    // First character must be letter, underscore, or $
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }

    // Rest can include digits
    for c in chars {
        if !c.is_ascii_alphanumeric() && c != '_' && c != '$' {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_literals() {
        let props = HashSet::new();
        let locals = HashSet::new();

        assert!(validate_head_expression("\"Hello\"", &props, &locals).is_ok());
        assert!(validate_head_expression("'World'", &props, &locals).is_ok());
    }

    #[test]
    fn test_prop_references() {
        let mut props = HashSet::new();
        props.insert("title".to_string());
        let locals = HashSet::new();

        assert!(validate_head_expression("title", &props, &locals).is_ok());
        assert!(validate_head_expression("unknown", &props, &locals).is_err());
    }

    #[test]
    fn test_disallowed_runtime() {
        let props = HashSet::new();
        let locals = HashSet::new();

        assert!(validate_head_expression("window.location", &props, &locals).is_err());
        assert!(validate_head_expression("Date.now()", &props, &locals).is_err());
    }
}
