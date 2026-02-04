#[test]
fn test_reproduction_docs_order() {
    use crate::codegen::{generate_runtime_code_internal, CodegenInput};
    use crate::validate::{ExpressionInput, TemplateNode};
    use std::collections::HashSet;

    let script_content = r#"
    import { zenCollection } from 'zenith:content';
    const createZenOrder = (typeof window !== 'undefined' ? (window as any).createZenOrder : null) || (() => ({ sections: [], getSectionBySlug: () => null, getDocBySlug: () => null }));
    
    state rawSections = zenCollection('documentation').sortBy('order', 'asc').groupByFolder().get()
    
    // The problematic line
    const docsOrder = createZenOrder(rawSections)
    
    function render() {
        const section = docsOrder.getSectionBySlug(selectedS);
    }
    
    zenEffect(render)
    "#;

    let mut expressions = Vec::new();
    // Simulate zenEffect(render)
    expressions.push(ExpressionInput {
        id: "expr_1".to_string(),
        code: "zenEffect(render)".to_string(),
        loop_context: None,
    });

    let nodes = vec![]; // Empty template

    let input = CodegenInput {
        file_path: "documentation/test.zen".to_string(),
        script_content: script_content.to_string(),
        expressions,
        styles: vec![],
        template_bindings: vec![],
        location: "test".to_string(),
        nodes,
        page_bindings: vec!["rawSections".to_string()],
        page_props: vec![],
        all_states: vec![("rawSections".to_string(), "zenCollection...".to_string())]
            .into_iter()
            .collect(),
        locals: vec![],
    };

    let result = generate_runtime_code_internal(input);
    let code = result.bundle;

    println!("Generated Code:\n{}", code);

    // Verify docsOrder is present
    assert!(
        code.contains("const docsOrder ="),
        "docsOrder declaration missing"
    );

    // Verify render uses docsOrder bare (not scope.locals.docsOrder)
    assert!(
        code.contains("docsOrder.getSectionBySlug"),
        "docsOrder usage incorrect"
    );

    // Verify expressions
    assert!(
        code.contains("zenEffect(render)"),
        "zenEffect expression missing"
    );
}
#[test]
fn test_is_inline_script_preservation() {
    use crate::parse::{parse_script, parse_template};
    use crate::validate::TemplateNode;

    let html = r#"
        <script setup>
            const state = 1;
        </script>
        <div></div>
        <script is:inline>
            const raw = 2;
        </script>
    "#;

    // 1. Verify parse_script IGNORES inline script
    let script_ir = parse_script(html).expect("Should find setup script");
    assert!(script_ir.raw.contains("const state = 1"));
    assert!(
        !script_ir.raw.contains("const raw = 2"),
        "parse_script should ignore is:inline"
    );

    // 2. Verify parse_template PRESERVES inline script
    let template_ir = parse_template(html, "test.zen").expect("Should parse template");

    // Find the inline script node
    let mut found_inline = false;
    for node in &template_ir.nodes {
        if let TemplateNode::Element(el) = node {
            if el.tag == "script" {
                // Check if it has content
                if let Some(TemplateNode::Text(text)) = el.children.first() {
                    if text.value.contains("const raw = 2") {
                        found_inline = true;
                    }
                }
            }
        }
    }

    assert!(
        found_inline,
        "parse_template should preserve inline script content"
    );
}
