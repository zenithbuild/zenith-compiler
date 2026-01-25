import { readFileSync } from 'fs'
import { parseTemplate } from './parse/parseTemplate'
import { parseScript } from './parse/parseScript'
import { transformTemplate } from './transform/transformTemplate'
import { finalizeOutputOrThrow } from './finalize/finalizeOutput'
import { validateIr } from './validate/invariants'
import type { ZenIR, StyleIR } from './ir/types'
import type { CompiledTemplate } from './output/types'
import type { FinalizedOutput } from './finalize/finalizeOutput'
import { parseZenFile } from './parseZenFile'
import { discoverComponents } from './discovery/componentDiscovery'
import { resolveComponentsInIR } from './transform/componentResolver'

/**
 * Compile a .zen file into IR and CompiledTemplate
 */
export async function compileZen(filePath: string): Promise<{
  ir: ZenIR
  compiled: CompiledTemplate
  finalized?: FinalizedOutput
}> {
  const source = readFileSync(filePath, 'utf-8')
  return compileZenSource(source, filePath)
}

/**
 * Compile Zen source string into IR and CompiledTemplate
 */
export async function compileZenSource(
  source: string,
  filePath: string,
  options?: {
    componentsDir?: string
    components?: Map<string, any> // ComponentMetadata
  }
): Promise<{
  ir: ZenIR
  compiled: CompiledTemplate
  finalized?: FinalizedOutput
}> {
  // Parse with native bridge
  let ir = parseZenFile(filePath, source)

  // Resolve components if explicitly provided OR if components directory is set
  if (options?.components || options?.componentsDir) {
    let components = options.components || new Map()

    // If directory provided, discover and merge
    if (options.componentsDir) {
      const discovered = discoverComponents(options.componentsDir)
      components = new Map([...components, ...discovered])
    }

    // Component resolution may throw InvariantError — let it propagate
    ir = resolveComponentsInIR(ir, components)
  }

  // Validate all compiler invariants after resolution
  // Throws InvariantError if any invariant is violated
  validateIr(ir)

  const compiled = transformTemplate(ir)

  try {
    const finalized = await finalizeOutputOrThrow(ir, compiled)
    return { ir, compiled, finalized }
  } catch (error: any) {
    throw new Error(`Failed to finalize output for ${filePath}:\n${error.message}`)
  }
}

export { parseZenFile }

// Feature exports
export { discoverComponents } from './discovery/componentDiscovery'
export { discoverLayouts } from './discovery/layouts'
export { processLayout } from './transform/layoutProcessor'
export { bundlePageScript } from './bundler'
export { buildSSG } from './ssg-build'
export { buildSPA } from './spa-build'
export { generateBundleJS } from './runtime/bundle-generator'
export { generateRouteDefinition } from '@zenithbuild/router/manifest'
export { compileCss, compileCssAsync, resolveGlobalsCss } from './css'
export { loadZenithConfig } from './core/config/loader'
export { PluginRegistry, createPluginContext, getPluginDataByNamespace } from './core/plugins/registry'
export {
  createBridgeAPI,
  runPluginHooks,
  collectHookReturns,
  buildRuntimeEnvelope,
  clearHooks
} from './core/plugins/bridge'
export type { HookContext } from './core/plugins/bridge'
export type { BundlePlan, ExpressionIR, ZenIR, TemplateNode } from './ir/types'
