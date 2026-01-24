import { expect, test, describe } from "bun:test";
import { analyzeAndEmit } from "../runtime/analyzeAndEmit";
import type { ZenIR } from "../ir/types";

function createIR(templateNodes: any[] = [], expressions: any[] = [], script: string = ""): ZenIR {
    return {
        filePath: "test.zen",
        template: {
            raw: "",
            nodes: templateNodes,
            expressions: expressions
        },
        script: {
            raw: script,
            attributes: {}
        },
        styles: [],
        componentScripts: []
    };
}

describe("Rust Compiler Authority", () => {
    test("JSX transformation into __zenith.h", async () => {
        const ir = createIR(
            [],
            [{ id: "expr_jsx", code: "<div><span>Hello</span></div>" }],
            "state count = 0;"
        );

        const result = await analyzeAndEmit(ir);
        const bundle = result.bundle;

        // Should contain native indicators (like state. prefix or h() calls)
        expect(bundle).toContain("state");

        // Should contain __zenith.h calls in expressions
        expect(result.expressions).toContain('__zenith.h("div"');
        expect(result.expressions).toContain('__zenith.h("span"');
        expect(result.expressions).toContain('"Hello"');

        // Should NOT contain raw HTML tags
        expect(result.expressions).not.toContain("<div>");
    });

    test("State identifier renaming - state. prefix", async () => {
        const ir = createIR(
            [],
            [{ id: "expr_state", code: "count + 1" }],
            "state count = 0;"
        );

        const result = await analyzeAndEmit(ir);

        // Should be renamed to state.count
        expect(result.expressions).toContain("state.count");
        // It shouldn't be plain count + 1 (except maybe in some comment if we had one)
        expect(result.expressions).not.toContain("return (count + 1)");
    });

    test("Local variables (loop vars) are NOT prefixed with state.", async () => {
        const ir = createIR(
            [
                {
                    type: "loop-fragment",
                    itemVar: "item",
                    indexVar: "i",
                    source: "items",
                    body: [],
                    location: { line: 1, column: 1 }
                }
            ],
            [{ id: "expr_loop", code: "item.name + i" }],
            "state items = [];"
        );

        const result = await analyzeAndEmit(ir);

        // item and i should STAY AS IS
        expect(result.expressions).toContain("item.name + i");
        expect(result.expressions).not.toContain("state.item");
        expect(result.expressions).not.toContain("state.i");
    });

    test("Nested JSX with state and local variables", async () => {
        const ir = createIR(
            [
                {
                    type: "loop-fragment",
                    itemVar: "item",
                    indexVar: "i",
                    source: "items",
                    body: [],
                    location: { line: 1, column: 1 }
                }
            ],
            [{ id: "expr_complex", code: "items.map(it => <div class={active ? 'active' : ''}>{it.name}</div>)" }],
            "state items = []; state active = true;"
        );

        const result = await analyzeAndEmit(ir);

        // 1. JSX lowered
        expect(result.expressions).toContain('__zenith.h("div"');

        // 2. state.active
        expect(result.expressions).toContain('state.active');

        // 2. state.active
        expect(result.expressions).toContain('class: state.active');

        // 3. state.items
        expect(result.expressions).toContain('state.items.map');

        // 4. it.name stays as is (it's local to the map)
        expect(result.expressions).toContain('[it.name]');
        expect(result.expressions).not.toContain(': state.it');
    });

    test("JSX Attributes transformation", async () => {
        const ir = createIR(
            [],
            [{ id: "expr_attr", code: "<button disabled={loading} onclick={() => count++}>Click</button>" }],
            "state loading = false; state count = 0;"
        );

        const result = await analyzeAndEmit(ir);

        expect(result.expressions).toContain('__zenith.h("button"');
        // Attributes should be in an object
        expect(result.expressions).toContain('disabled: state.loading');
        expect(result.expressions).toContain('onclick: () => state.count++');
    });
});
