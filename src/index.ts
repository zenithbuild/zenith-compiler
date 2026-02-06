// Core Imports (Internal)
import { parseZenFile } from './parseZenFile'
import { InvariantError } from './errors/compilerError'

// Essential Type Exports
import type { ZenIR } from './ir/types'
import type { CompiledTemplate, FinalizedOutput } from './output/types'

export type ZenCompileOptions = {
  /**
   * Map of component names to their definitions.
   */
  components?: Map<string, any>
  /**
   * Optional layout to wrap the page in
   */
  layout?: any
  /**
   * Initial props for layout processing
   */
  props?: Record<string, any>
}

export type ZenCompileResult = {
  ir: ZenIR
  compiled: CompiledTemplate
  finalized?: FinalizedOutput
}

/**
 * Compile Zenith source code using the unified "One True Bridge" (Native Syscall).
 */
export async function compile(
  source: string,
  filePath: string,
  options?: ZenCompileOptions
): Promise<ZenCompileResult> {
  const opts = options || {};
  const components = opts.components || new Map()

  const finalized = parseZenFile(filePath, source, {
    mode: 'full',
    components: components ? Object.fromEntries(components) : {},
    layout: opts.layout,
    props: opts.props,
    useCache: true
  });

  // If the native side returned a compiler error (camelCase or snake_case fields)
  if (finalized.code && (finalized.errorType || finalized.error_type)) {
    throw new InvariantError(
      finalized.code,
      finalized.message,
      finalized.guarantee || "Zenith Invariant Violation", // Guarantee provided by native code checks
      filePath,
      finalized.line || 1,
      finalized.column || 1,
      finalized.context,
      finalized.hints
    );
  }

  return {
    ir: finalized.ir as ZenIR,
    compiled: {
      html: finalized.html,
      bindings: finalized.bindings || [],
      scripts: finalized.js || null,
      styles: finalized.styles || []
    },
    finalized: {
      ...finalized,
      js: finalized.js,
      npmImports: finalized.npmImports,
      bundlePlan: finalized.bundlePlan
    }
  };
}

export * from './core'
export { parseZenFile }
export type { FinalizedOutput, CompiledTemplate }
export type { BundlePlan, ExpressionIR, ZenIR, TemplateNode } from './ir/types'
export type { HookContext } from './core/plugins/bridge'

