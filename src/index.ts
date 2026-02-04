// Core Imports (Internal)
import { parseZenFile } from './parseZenFile'
import { InvariantError } from './errors/compilerError'
import { discoverComponents } from './discovery/componentDiscovery'

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
  const finalized = parseZenFile(filePath, source, {
    mode: 'full',
    components: options?.components ? Object.fromEntries(options.components) : {},
    layout: options?.layout,
    props: options?.props,
    useCache: true
  });

  // If the native side returned a compiler error (camelCase fields)
  if (finalized.code && finalized.errorType) {
    throw new InvariantError(
      finalized.code,
      finalized.message,
      "Zenith Invariant Violation", // Guarantee provided by native code checks
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

/**
 * Compile Zen source string into IR and CompiledTemplate.
 * 
 * This is the primary entry point for the CLI dev server.
 * It discovers components from componentsDir and passes them to the native
 * compiler for resolution, ensuring scope registration is properly emitted.
 */
export async function compileZenSource(
  source: string,
  filePath: string,
  options?: {
    componentsDir?: string
    components?: Map<string, any>
  }
): Promise<ZenCompileResult> {
  // Discover components if directory provided
  let components = options?.components || new Map()

  if (options?.componentsDir) {
    const discovered = discoverComponents(options.componentsDir)
    components = new Map([...components, ...discovered])
  }

  // Convert components to object format expected by parseZenFile
  const componentsObj: Record<string, any> = {}
  for (const [name, meta] of components) {
    componentsObj[name] = meta
  }

  // Call parseZenFile with components - native compiler handles resolution
  const finalized = parseZenFile(filePath, source, {
    mode: 'full',
    components: componentsObj,
    useCache: false // CLI dev mode should not cache
  });

  // If the native side returned a compiler error
  if (finalized.code && finalized.errorType) {
    throw new InvariantError(
      finalized.code,
      finalized.message,
      "Zenith Invariant Violation",
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

export { parseZenFile }
export type { FinalizedOutput, CompiledTemplate }
export type { BundlePlan, ExpressionIR, ZenIR, TemplateNode } from './ir/types'

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

