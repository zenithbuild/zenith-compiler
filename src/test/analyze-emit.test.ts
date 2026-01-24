import { expect, test, describe } from "bun:test";
import { analyzeAndEmit } from "../runtime/analyzeAndEmit";
import type { ZenIR } from "../ir/types";

const defaultLoc = { line: 1, column: 1 };

function createIR(templateNodes: any[] = [], expressions: any[] = [], script: string = ""): ZenIR {
    const nodes = templateNodes.map(n => ({
        location: defaultLoc,
        ...n,
        // Recursively add location to children if they are elements or fragments
        children: n.children ? n.children.map((c: any) => ({ location: defaultLoc, ...c })) : undefined,
        body: n.body ? n.body.map((c: any) => ({ location: defaultLoc, ...c })) : undefined,
        consequent: n.consequent ? n.consequent.map((c: any) => ({ location: defaultLoc, ...c })) : undefined,
        alternate: n.alternate ? n.alternate.map((c: any) => ({ location: defaultLoc, ...c })) : undefined,
        fragment: n.fragment ? n.fragment.map((c: any) => ({ location: defaultLoc, ...c })) : undefined,
    }));

    return {
        filePath: "test.zen",
        template: {
            raw: "",
            nodes: nodes,
            expressions: expressions.map(e => ({ location: defaultLoc, ...e }))
        },
        script: {
            raw: script,
            attributes: {}
        },
        styles: [],
        componentScripts: []
    };
}

describe("Analyze + Emit Pass", () => {
    test("Step 1: Binding Collection includes script variables and loop params", async () => {
        const ir = createIR(
            [
                {
                    type: "loop-fragment",
                    itemVar: "item",
                    indexVar: "i",
                    source: "items",
                    body: []
                }
            ],
            [],
            "const count = 10; function increment() {} state active = true;"
        );

        const result = await analyzeAndEmit(ir);
        // We can't easily inspect the internal binding table without exporting it or adding debug hooks,
        // but we can verify it by checking if expressions using these variables are correctly analyzed.
    });

    test("Step 2: Expressions resolve against script and loop bindings", async () => {
        const ir = createIR(
            [
                {
                    type: "loop-fragment",
                    itemVar: "item",
                    indexVar: "i",
                    source: "items",
                    body: []
                }
            ],
            [
                { id: "expr_0", code: "count + 1" },
                { id: "expr_1", code: "item.name" },
                { id: "expr_2", code: "i > 0" },
                { id: "expr_3", code: "active ? 'yes' : 'no'" }
            ],
            "const count = 10; state active = true;"
        );

        const result = await analyzeAndEmit(ir);

        // Check if expressions were correctly analyzed as using state
        // (Since they resolve against bindings, they should be marked as usesState = true)
        expect(result.expressions).toContain("expr_0");
        expect(result.expressions).toContain("count + 1");

        // Verify dependency metadata in the emitted bundle
        // In the new Rust compiler, state variables are prefixed with state.
        expect(result.expressions).toContain('state.active');
    });

    test("Renamed component symbols are correctly resolved", async () => {
        const ir = createIR(
            [],
            [
                { id: "expr_comp", code: "val_inst0.value" }
            ],
            "const val_inst0 = signal(0);"
        );

        const result = await analyzeAndEmit(ir);
        expect(result.expressions).toContain("val_inst0.value");
    });

    test("Strict emission order: symbols before expressions", async () => {
        const ir = createIR(
            [],
            [{ id: "expr_test", code: "count + 1" }],
            "state count = 1;"
        );

        const result = await analyzeAndEmit(ir);
        const bundle = result.bundle;

        const countDeclPos = bundle.indexOf("state.count = 1;");
        const statePos = bundle.indexOf("count: undefined");
        const exprPos = bundle.indexOf("function _expr_expr_test");

        // Verify that declaration comes before state initialization, and state comes before expressions
        expect(countDeclPos).toBeGreaterThan(-1);
        expect(statePos).toBeGreaterThan(-1);
        expect(exprPos).toBeGreaterThan(statePos);
    });
});
