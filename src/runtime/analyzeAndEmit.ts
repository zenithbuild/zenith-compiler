/**
 * Transform IR to Runtime Code
 * 
 * NATIVE BRIDGE: Delegates ALL compilation to Rust (`native/compiler-native/src/codegen.rs`).
 */

import type { ZenIR, TemplateNode } from '../ir/types'

let native: any;
try {
  try {
    native = require('../../native/compiler-native');
  } catch {
    native = require('../../native/compiler-native/index.js');
  }
} catch (e) {
  // Bridge load handled elsewhere
}

export interface RuntimeCode {
  expressions: string
  render: string
  hydration: string
  styles: string
  script: string
  stateInit: string
  bundle: string
  npmImports: any[]
}

/**
 * Analyze ZenIR and emit runtime JavaScript code via Native Bridge
 */
export async function analyzeAndEmit(ir: ZenIR): Promise<RuntimeCode> {
  const scriptContent = ir.script?.raw || ''

  if (!native || !native.generateRuntimeCode) {
    throw new Error(`[Zenith Native] Runtime codegen bridge unavailable`);
  }

  const codegenInput = {
    filePath: ir.filePath,
    scriptContent: scriptContent,
    expressions: ir.template.expressions,
    styles: ir.styles.map(s => ({ raw: s.raw })),
    templateBindings: collectTemplateBindings(ir.template.nodes),
    location: ir.filePath,
    nodes: ir.template.nodes,
    pageBindings: ir.pageBindings || [],
  };

  try {
    return native.generateRuntimeCode(JSON.stringify(codegenInput));
  } catch (e: any) {
    throw new Error(`[Zenith Native] Codegen failed: ${e.message}`);
  }
}

/**
 * Collect template bindings (loop item/index variables) for native codegen
 */
function collectTemplateBindings(nodes: TemplateNode[]): string[] {
  const bindings: string[] = [];
  function walk(nodeList: TemplateNode[]) {
    for (const node of nodeList) {
      if (node.type === 'loop-fragment') {
        bindings.push(node.itemVar);
        if (node.indexVar) bindings.push(node.indexVar);
        walk(node.body);
      } else if (node.type === 'element') {
        walk(node.children);
      } else if (node.type === 'conditional-fragment') {
        walk(node.consequent);
        walk(node.alternate);
      } else if (node.type === 'optional-fragment') {
        walk(node.fragment);
      }
    }
  }
  walk(nodes);
  return bindings;
}
