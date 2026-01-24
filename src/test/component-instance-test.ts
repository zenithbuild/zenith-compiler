
import { test, expect } from "bun:test";
import { resolveComponentsInIR } from "../transform/componentResolver";
import type { ZenIR, TemplateNode, ExpressionIR } from "../ir/types";
import type { ComponentMetadata } from "../discovery/componentDiscovery";

// Helper to create a basic IR
function createIR(nodes: TemplateNode[], expressions: ExpressionIR[] = []): ZenIR {
    return {
        filePath: "test.zen",
        template: {
            raw: "",
            nodes,
            expressions
        },
        script: { raw: "", attributes: {} },
        styles: [],
        componentScripts: []
    };
}

// Helper to create component metadata
function createComponent(name: string, nodes: TemplateNode[], expressions: ExpressionIR[]): ComponentMetadata {
    return {
        name,
        path: `${name}.zen`,
        template: "",
        nodes,
        expressions,
        slots: [],
        props: [],
        styles: [],
        script: null,
        scriptAttributes: null,
        hasScript: false,
        hasStyles: false
    };
}

test("Component instantiation generates unique instance expression IDs", () => {
    // Define a component "Card" with one expression {title}
    // usage: <div>{title}</div>
    const cardExpr: ExpressionIR = { id: "expr_card_1", code: "title", location: { line: 1, column: 1 } };
    const cardNodes: TemplateNode[] = [
        {
            type: "element",
            tag: "div",
            attributes: [],
            children: [
                {
                    type: "expression",
                    expression: "expr_card_1",
                    location: { line: 1, column: 1 }
                }
            ],
            location: { line: 1, column: 1 }
        }
    ];

    const components = new Map<string, ComponentMetadata>();
    components.set("Card", createComponent("Card", cardNodes, [cardExpr]));

    // Create a page using <Card /> twice
    // <root>
    //   <Card />
    //   <Card />
    // </root>
    const pageNodes: TemplateNode[] = [
        {
            type: "element",
            tag: "root",
            attributes: [],
            children: [
                {
                    type: "component",
                    name: "Card",
                    attributes: [],
                    children: [],
                    location: { line: 1, column: 1 }
                },
                {
                    type: "component",
                    name: "Card",
                    attributes: [],
                    children: [],
                    location: { line: 2, column: 1 }
                }
            ],
            location: { line: 0, column: 0 }
        }
    ];

    const ir = createIR(pageNodes);

    // Resolve components
    const resolvedIR = resolveComponentsInIR(ir, components);

    // Assertions
    const root = resolvedIR.template.nodes[0] as any;
    expect(root.type).toBe("element");
    expect(root.children.length).toBe(2);

    const card1 = root.children[0];
    const card2 = root.children[1];

    // Check structure
    expect(card1.type).toBe("element"); // Resolved to div
    expect(card2.type).toBe("element");

    // Check expression nodes inside
    const exprNode1 = card1.children[0];
    const exprNode2 = card2.children[0];

    expect(exprNode1.type).toBe("expression");
    expect(exprNode2.type).toBe("expression");

    const id1 = exprNode1.expression;
    const id2 = exprNode2.expression;

    console.log(`Card 1 Expression ID: ${id1}`);
    console.log(`Card 2 Expression ID: ${id2}`);

    // Critically, IDs must be unique
    expect(id1).not.toBe(id2);
    expect(id1).not.toBe("expr_card_1"); // Should be rewritten
    expect(id1).toContain("inst");

    // Check registry
    const registryIDs = resolvedIR.template.expressions.map(e => e.id);
    expect(registryIDs).toContain(id1);
    expect(registryIDs).toContain(id2);

    // Ensure both are registered
    expect(registryIDs.length).toBeGreaterThanOrEqual(2);
});
