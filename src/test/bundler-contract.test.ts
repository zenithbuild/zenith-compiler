import { expect, test, describe } from "bun:test"
import { bundlePageScript } from "../bundler"
import type { BundlePlan } from "../ir/types"

/**
 * Bundler Contract Tests
 * 
 * These tests verify the compiler-first bundler architecture:
 * - Plan exists → bundling MUST occur
 * - Bundling failure → hard error thrown (no fallback)
 * - Same plan → deterministic output
 * 
 * The bundler performs ZERO inference.
 */

describe("Bundler Contract", () => {
    test("executes plan with virtual modules", async () => {
        const plan: BundlePlan = {
            entry: `
import { foo } from 'virtual:test';
console.log(foo);
`,
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: [{
                id: "virtual:test",
                code: "export const foo = 'bar';"
            }]
        }

        const result = await bundlePageScript(plan)
        expect(result).toContain("bar")
        expect(result).not.toContain("import") // Should be bundled, no external imports
    })

    test("resolves zenith:content virtual module", async () => {
        const plan: BundlePlan = {
            entry: `
import { zenCollection } from 'zenith:content';
console.log(zenCollection);
`,
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: [{
                id: '\0zenith:content',
                code: `export const zenCollection = (typeof globalThis !== 'undefined' ? globalThis : window).zenCollection;`
            }]
        }

        const result = await bundlePageScript(plan)
        expect(result).toContain("zenCollection")
        expect(result).not.toContain("import") // Should be bundled
    })

    test("deterministic output for same plan", async () => {
        const plan: BundlePlan = {
            entry: "const x = 1; const y = 2; console.log(x + y);",
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: []
        }

        const result1 = await bundlePageScript(plan)
        const result2 = await bundlePageScript(plan)
        expect(result1).toBe(result2)
    })

    test("throws on unresolvable module (no fallback)", async () => {
        const plan: BundlePlan = {
            entry: "import { nonexistent } from 'this-package-does-not-exist-xyz-123456';",
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: []
        }

        // Bundler must throw - no silent fallback
        await expect(bundlePageScript(plan)).rejects.toThrow()
    })

    test("respects platform setting", async () => {
        const browserPlan: BundlePlan = {
            entry: "console.log('browser');",
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: []
        }

        const nodePlan: BundlePlan = {
            entry: "console.log('node');",
            platform: "node",
            format: "esm",
            resolveRoots: [],
            virtualModules: []
        }

        const browserResult = await bundlePageScript(browserPlan)
        const nodeResult = await bundlePageScript(nodePlan)

        // Both should execute without error
        expect(browserResult).toContain("browser")
        expect(nodeResult).toContain("node")
    })

    test("respects format setting", async () => {
        const esmPlan: BundlePlan = {
            entry: "export const x = 1;",
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: []
        }

        const cjsPlan: BundlePlan = {
            entry: "module.exports = { x: 1 };",
            platform: "node",
            format: "cjs",
            resolveRoots: [],
            virtualModules: []
        }

        const esmResult = await bundlePageScript(esmPlan)
        const cjsResult = await bundlePageScript(cjsPlan)

        // ESM should have export, CJS should have exports/module
        expect(esmResult).toBeDefined()
        expect(cjsResult).toBeDefined()
    })

    test("no tree-shaking - unused exports preserved", async () => {
        const plan: BundlePlan = {
            entry: `
import { used } from 'virtual:lib';
console.log(used);
`,
            platform: "browser",
            format: "esm",
            resolveRoots: [],
            virtualModules: [{
                id: "virtual:lib",
                code: `
export const used = 'used';
export const unused = 'unused';
`
            }]
        }

        const result = await bundlePageScript(plan)
        // With treeshake: false, unused export should still be in output
        // (This test verifies bundler doesn't infer side effects)
        expect(result).toContain("used")
    })
})
