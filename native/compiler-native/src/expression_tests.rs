#[cfg(test)]
mod tests {
    use crate::transform::*;
    use std::collections::HashSet;

    #[test]
    fn test_primitive_expression_evaluation() {
        let code = "count + 1".to_string();
        let id = "expr_1".to_string();
        let mut bindings = HashSet::new();
        bindings.insert("count".to_string());
        let bindings_json = serde_json::to_string(&bindings).unwrap();

        let result = evaluate_expression_native(id, code, bindings_json, None);
        
        assert_eq!(result.expression_type, "primitive");
        assert!(result.dependencies.contains(&"count".to_string()));
        assert!(result.uses_state);
        assert!(!result.uses_loop_context);
    }

    #[test]
    fn test_ternary_jsx_evaluation() {
        let code = "isActive ? <On /> : <Off />".to_string();
        let id = "expr_2".to_string();
        let mut bindings = HashSet::new();
        bindings.insert("isActive".to_string());
        let bindings_json = serde_json::to_string(&bindings).unwrap();

        let result = evaluate_expression_native(id, code, bindings_json, None);
        
        assert_eq!(result.expression_type, "conditional");
        assert!(result.dependencies.contains(&"isActive".to_string()));
        
        let classification: ExpressionClassification = serde_json::from_str(&result.classification_json).unwrap();
        assert_eq!(classification.condition.unwrap(), "isActive");
        assert_eq!(classification.consequent.unwrap(), "<On />");
        assert_eq!(classification.alternate.unwrap(), "<Off />");
    }

    #[test]
    fn test_map_jsx_evaluation() {
        let code = "items.map(item => <li key={item.id}>{item.name}</li>)".to_string();
        let id = "expr_3".to_string();
        let mut bindings = HashSet::new();
        bindings.insert("items".to_string());
        let bindings_json = serde_json::to_string(&bindings).unwrap();

        let result = evaluate_expression_native(id, code, bindings_json, None);
        
        assert_eq!(result.expression_type, "loop");
        assert!(result.dependencies.contains(&"items".to_string()));
        
        let classification: ExpressionClassification = serde_json::from_str(&result.classification_json).unwrap();
        assert_eq!(classification.loop_source.unwrap(), "items");
        assert_eq!(classification.loop_item_var.unwrap(), "item");
        assert!(classification.loop_body.unwrap().contains("<li"));
    }
}
