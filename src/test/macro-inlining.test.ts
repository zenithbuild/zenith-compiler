import { test, expect } from "bun:test";
import { resolveComponentsInIR } from "../transform/componentResolver";
import type { ZenIR, TemplateNode } from "../ir/types";
import type { ComponentMetadata } from "../discovery/componentDiscovery";

// Helper to create a basic ZenIR
function createIR(templateContent: string, script: string = ''): ZenIR {
    return {
        filePath: 'test.zen',
        template: {
            raw: templateContent,
            nodes: [],
            expressions: []
        },
        script: {
            raw: script,
            attributes: {}
        },
        styles: [],
        componentScripts: []
    } as any;
}

test("Selective symbol renaming in component expressions", async () => {
    // 1. Mock component metadata
    const counterMeta: any = {
        name: "Counter",
        hasScript: true,
        script: "let count = 0; function inc() { count++; }",
        expressions: [
            { id: "expr_0", code: "count" },
            { id: "expr_1", code: "Math.floor(count / 2)" }
        ],
        nodes: [
            {
                type: 'element',
                tag: 'button',
                attributes: [{ name: 'onclick', value: { id: 'expr_1', code: 'inc()', location: { line: 1, column: 1 } }, location: { line: 1, column: 1 } }],
                children: [{ type: 'expression', expression: 'expr_0', location: { line: 1, column: 1 } }],
                location: { line: 1, column: 1 }
            }
        ],
        props: [],
        styles: []
    };

    const components = new Map<string, any>();
    components.set("Counter", counterMeta);

    // 2. Mock page IR with two counter instances
    const pageIR = createIR("<Counter /><Counter />");
    pageIR.template.nodes = [
        { type: 'component', name: 'Counter', attributes: [], children: [], location: { line: 1, column: 1 } },
        { type: 'component', name: 'Counter', attributes: [], children: [], location: { line: 1, column: 10 } }
    ];

    // 3. Resolve components
    const resolvedIR = resolveComponentsInIR(pageIR, components);

    // 4. Verify expressions are renamed
    // We expect 4 expressions (2 per instance)
    expect(resolvedIR.template.expressions.length).toBe(4);

    // Check first instance
    const expr0_inst0 = resolvedIR.template.expressions.find(e => e.id === "expr_0_inst0");
    expect(expr0_inst0?.code).toBe("count_inst0");

    // Check second instance
    const expr0_inst1 = resolvedIR.template.expressions.find(e => e.id === "expr_0_inst1");
    expect(expr0_inst1?.code).toBe("count_inst1");

    // 5. Verify script renaming
    expect(resolvedIR.script?.raw).toContain("let count_inst0 = 0");
    expect(resolvedIR.script?.raw).toContain("function inc_inst0()");
    expect(resolvedIR.script?.raw).toContain("let count_inst1 = 0");
});

test("Parent symbols are NOT renamed in component expressions", async () => {
    const compMeta: any = {
        name: "Hello",
        hasScript: true,
        script: "let local = 'hi';",
        expressions: [
            { id: "expr_0", code: "local + globalVar" }
        ],
        nodes: [],
        props: [],
        styles: []
    };

    const components = new Map<string, any>();
    components.set("Hello", compMeta);

    const pageIR = createIR("<Hello />");
    pageIR.template.nodes = [{ type: 'component', name: 'Hello', attributes: [], children: [], location: { line: 1, column: 1 } }];

    const resIR = resolveComponentsInIR(pageIR, components);

    const expr = resIR.template.expressions[0];
    // 'local' should be renamed, 'globalVar' should NOT
    expect(expr?.code).toContain("_inst");
    expect(expr?.code).toContain("globalVar");
    expect(expr?.code).not.toContain("globalVar_inst");
});

test("Macro Prop Substitution", async () => {
    const compMeta: any = {
        name: "Welcome",
        hasScript: true,
        script: "console.log(title)",
        expressions: [
            { id: "expr_0", code: "title" }
        ],
        nodes: [],
        props: ["title"],
        styles: []
    };

    const components = new Map<string, any>();
    components.set("Welcome", compMeta);

    const pageIR = createIR("<Welcome title={pageTitle} />");
    pageIR.template.nodes = [{
        type: 'component',
        name: 'Welcome',
        attributes: [{
            name: 'title',
            value: { id: 'expr_P', code: 'pageTitle', location: { line: 1, column: 1 } },
            location: { line: 1, column: 1 }
        }],
        children: [],
        location: { line: 1, column: 1 }
    }];

    const resolvedIR = resolveComponentsInIR(pageIR, components);

    // Expression should be substituted with parent code
    const expr = resolvedIR.template.expressions.find(e => e.id === "expr_0_inst0");
    expect(expr?.code).toBe("(pageTitle)");

    // Script should also be substituted
    expect(resolvedIR.script?.raw).toContain("console.log((pageTitle))");
});

test("Event Handler Renaming in Template", async () => {
    const compMeta: any = {
        name: "Button",
        hasScript: true,
        script: "function handleClick() { console.log('clicked'); }",
        expressions: [],
        nodes: [
            {
                type: 'element',
                tag: 'button',
                attributes: [
                    { name: 'onclick', value: 'handleClick', location: { line: 1, column: 1 } }
                ],
                children: [],
                location: { line: 1, column: 1 }
            }
        ],
        props: [],
        styles: []
    };

    const components = new Map<string, any>();
    components.set("Button", compMeta);

    const pageIR = createIR("<Button />");
    pageIR.template.nodes = [{ type: 'component', name: 'Button', attributes: [], children: [], location: { line: 1, column: 1 } }];

    const resolvedIR = resolveComponentsInIR(pageIR, components);

    // The button's onclick should be renamed
    const button: any = resolvedIR.template.nodes[0];
    expect(button.attributes[0].value).toBe("handleClick_inst0");
});

test("Multi-instance isolation and props.prefix substitution", async () => {
    const compMeta: any = {
        name: "Box",
        hasScript: true,
        script: "const val = signal(props.initial);",
        expressions: [{ id: "expr_0", code: "val.value + props.label" }],
        nodes: [],
        props: ["initial", "label"],
        styles: []
    };

    const components = new Map<string, any>();
    components.set("Box", compMeta);

    const pageIR = createIR("<Box initial={1} label='A' /><Box initial={10} label='B' />");
    pageIR.template.nodes = [
        { type: 'component', name: 'Box', attributes: [{ name: 'initial', value: { id: 'expr_P1', code: '1' } }, { name: 'label', value: 'A' }], children: [], location: { line: 1, column: 1 } },
        { type: 'component', name: 'Box', attributes: [{ name: 'initial', value: { id: 'expr_P2', code: '10' } }, { name: 'label', value: 'B' }], children: [], location: { line: 1, column: 30 } }
    ] as any;

    const resolvedIR = resolveComponentsInIR(pageIR, components);

    // Verify first instance
    const script = resolvedIR.script?.raw || "";
    expect(script).toContain("const val_inst0 = signal((1))");
    const expr0 = resolvedIR.template.expressions.find(e => e.id === "expr_0_inst0");
    expect(expr0?.code).toBe("val_inst0.value + \"A\"");

    // Verify second instance
    expect(script).toContain("const val_inst1 = signal((10))");
    const expr1 = resolvedIR.template.expressions.find(e => e.id === "expr_0_inst1");
    expect(expr1?.code).toBe("val_inst1.value + \"B\"");
});
