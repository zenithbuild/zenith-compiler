/**
 * Finalize Output
 * 
 * NATIVE BRIDGE: Delegates ALL finalization (Phase 8/9/10) to Rust (`native/compiler-native/src/finalize.rs`).
 */

import type { CompiledTemplate } from '../output/types'
import type { ZenIR, BundlePlan } from '../ir/types'

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

/**
 * Finalized output ready for browser
 */
export interface FinalizedOutput {
  html: string
  js: string
  npmImports: string // Hoisted imports
  styles: string[]
  hasErrors: boolean
  errors: string[]
  /** Compiler-emitted bundling plan. If present, bundling MUST occur. If absent, bundling MUST NOT occur. */
  bundlePlan?: BundlePlan
}

/**
 * Finalize compiler output (Native Bridge)
 * 
 * @param ir - Intermediate representation
 * @param compiled - Compiled template
 * @returns Finalized output
 */
export async function finalizeOutput(
  ir: ZenIR,
  compiled: CompiledTemplate
): Promise<FinalizedOutput> {
  if (native && native.finalizeOutputNative) {
    try {
      const result = native.finalizeOutputNative(ir, compiled);

      return {
        html: result.html,
        js: result.js,
        npmImports: result.npmImports,
        styles: result.styles,
        hasErrors: result.hasErrors,
        errors: result.errors,
        bundlePlan: result.bundlePlan
      };
    } catch (error: any) {
      return {
        html: '',
        js: '',
        npmImports: '',
        styles: [],
        hasErrors: true,
        errors: [`Native finalization failed: ${error.message}`]
      };
    }
  }

  throw new Error(`[Zenith Native] Finalize bridge unavailable - cannot finalize ${ir.filePath}`)
}

/**
 * Generate final output with error handling
 */
export async function finalizeOutputOrThrow(
  ir: ZenIR,
  compiled: CompiledTemplate
): Promise<FinalizedOutput> {
  const output = await finalizeOutput(ir, compiled)

  if (output.hasErrors) {
    const errorMessage = output.errors.join('\n\n')
    throw new Error(`Compilation failed:\n\n${errorMessage}`)
  }

  return output
}
