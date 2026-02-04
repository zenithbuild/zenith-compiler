#[cfg(test)]
mod tests {
    use crate::transform::lower_fragments_native;
    use crate::validate::{ExpressionIR, SourceLocation, TemplateNode};
    use serde_json::json;

    fn mock_loc() -> SourceLocation {
        SourceLocation { line: 1, column: 1 }
    }

    #[test]
    fn test_conditional_lowering() {
        let nodes = vec![TemplateNode::Expression(crate::validate::ExpressionNode {
            expression: "expr1".to_string(),
            location: mock_loc(),
            loop_context: None,
            is_in_head: false,
        })];
        let expressions = vec![ExpressionIR {
            id: "expr1".to_string(),
            code: "isActive ? <div>Active</div> : <span>Inactive</span>".to_string(),
            location: mock_loc(),
            loop_context: None,
        }];

        let nodes_json = serde_json::to_string(&nodes).unwrap();
        let expressions_json = serde_json::to_string(&expressions).unwrap();

        let result_json =
            lower_fragments_native(nodes_json, expressions_json, "test.zen".to_string()).unwrap();
        let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();

        let out_nodes = result["nodes"].as_array().unwrap();
        assert_eq!(out_nodes.len(), 1);

        // Should be a ConditionalFragment
        assert_eq!(out_nodes[0]["type"], "conditional-fragment");
        assert_eq!(out_nodes[0]["consequent"][0]["type"], "element");
        assert_eq!(out_nodes[0]["consequent"][0]["tag"], "div");
        assert_eq!(out_nodes[0]["alternate"][0]["type"], "element");
        assert_eq!(out_nodes[0]["alternate"][0]["tag"], "span");
    }

    #[test]
    fn test_loop_lowering_with_context() {
        let nodes = vec![TemplateNode::Expression(crate::validate::ExpressionNode {
            expression: "expr1".to_string(),
            location: mock_loc(),
            loop_context: None,
            is_in_head: false,
        })];
        let expressions = vec![ExpressionIR {
            id: "expr1".to_string(),
            code: "items.map(item => <div class={item.className}>{item.text}</div>)".to_string(),
            location: mock_loc(),
            loop_context: None,
        }];

        let result_json = lower_fragments_native(
            serde_json::to_string(&nodes).unwrap(),
            serde_json::to_string(&expressions).unwrap(),
            "test.zen".to_string(),
        )
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();

        let out_nodes = result["nodes"].as_array().unwrap();
        assert_eq!(out_nodes[0]["type"], "loop-fragment");
        assert_eq!(out_nodes[0]["itemVar"], "item");

        let body = out_nodes[0]["body"].as_array().unwrap();
        assert_eq!(body[0]["type"], "element");
        assert_eq!(body[0]["tag"], "div");

        // Check reactive propagation
        let loop_ctx = body[0]["loopContext"].as_object().unwrap();
        assert!(loop_ctx["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "item"));
    }

    #[test]
    fn test_optional_lowering() {
        let nodes = vec![TemplateNode::Expression(crate::validate::ExpressionNode {
            expression: "expr1".to_string(),
            location: mock_loc(),
            loop_context: None,
            is_in_head: false,
        })];
        let expressions = vec![ExpressionIR {
            id: "expr1".to_string(),
            code: "show && <div>Optional</div>".to_string(),
            location: mock_loc(),
            loop_context: None,
        }];

        let result_json = lower_fragments_native(
            serde_json::to_string(&nodes).unwrap(),
            serde_json::to_string(&expressions).unwrap(),
            "test.zen".to_string(),
        )
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(&result_json).unwrap();

        let out_nodes = result["nodes"].as_array().unwrap();
        assert_eq!(out_nodes[0]["type"], "optional-fragment");
        assert_eq!(out_nodes[0]["fragment"][0]["type"], "element");
    }
}
