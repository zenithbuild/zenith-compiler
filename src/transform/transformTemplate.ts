/**
 * Transform Template IR to Compiled Template
 * 
 * Phase 2: Transform IR → Static HTML + Runtime Bindings
 */

import type { ZenIR } from '../ir/types'
import type { CompiledTemplate } from '../output/types'


/**
 * Transform a ZenIR into CompiledTemplate
 */
export function transformTemplate(ir: ZenIR): CompiledTemplate {
  let native: any
  try {
    try {
      native = require('../../native/compiler-native')
    } catch {
      native = require('../../native/compiler-native/index.js')
    }
  } catch (e) {
    // Bridge load handled elsewhere
  }

  if (native && native.transformTemplateNative) {
    const { html, bindings } = native.transformTemplateNative(
      JSON.stringify(ir.template.nodes),
      JSON.stringify(ir.template.expressions)
    )

    return {
      html,
      bindings,
      scripts: ir.script ? ir.script.raw : null,
      styles: ir.styles.map(s => s.raw)
    }
  }

  // Fallback to legacy if bridge unavailable (though we aim for full native)
  throw new Error('[Zenith Native] Transformation bridge unavailable')
}

