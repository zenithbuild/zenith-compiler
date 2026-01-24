/**
 * Component Discovery
 * 
 * Discovers and catalogs components in a Zenith project
 * Similar to layout discovery but for reusable components
 */

import * as fs from 'fs'
import * as path from 'path'
import type { TemplateNode, ExpressionIR } from '../ir/types'

export interface SlotDefinition {
    name: string | null  // null = default slot, string = named slot
    location: {
        line: number
        column: number
    }
}

export interface ComponentMetadata {
    name: string          // Component name (e.g., "Card", "Button")
    path: string          // Absolute path to .zen file
    template: string      // Raw template HTML
    nodes: TemplateNode[] // Parsed template nodes
    expressions: ExpressionIR[] // Component-level expressions
    slots: SlotDefinition[]
    props: string[]       // Declared props
    styles: string[]      // Raw CSS from <style> blocks
    script: string | null         // Raw script content for bundling
    scriptAttributes: Record<string, string> | null  // Script attributes (setup, lang)
    hasScript: boolean
    hasStyles: boolean
}


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
 * Discover all components in a directory
 * @param baseDir - Base directory to search (e.g., src/components)
 * @returns Map of component name to metadata
 */
export function discoverComponents(baseDir: string): Map<string, ComponentMetadata> {
    if (native && native.discoverComponentsNative) {
        try {
            const raw = native.discoverComponentsNative(baseDir)
            // The native function returns a Map-like object or Record
            const components = new Map<string, ComponentMetadata>()
            for (const [name, metadata] of Object.entries(raw)) {
                components.set(name, metadata as ComponentMetadata)
            }
            return components
        } catch (error: any) {
            console.warn(`[Zenith Native] Discovery failed for ${baseDir}: ${error.message}`)
        }
    }

    // Fallback or empty if native fails (bridge is required for performance)
    return new Map<string, ComponentMetadata>()
}

/**
 * Use native bridge for tag name checks
 */
export function isComponentTag(tagName: string): boolean {
    if (native && native.isComponentTagNative) {
        return native.isComponentTagNative(tagName)
    }
    return tagName.length > 0 && tagName[0] === tagName[0]?.toUpperCase()
}

/**
 * Get component metadata by name
 */
export function getComponent(
    components: Map<string, ComponentMetadata>,
    name: string
): ComponentMetadata | undefined {
    return components.get(name)
}

