import * as fs from 'fs'
import * as path from 'path'
export interface LayoutMetadata {
    name: string
    filePath: string
    props: string[]
    states: Map<string, string>
    html: string
    scripts: string[]
    styles: string[]
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
 * Discover layouts in a directory
 */
export function discoverLayouts(layoutsDir: string): Map<string, LayoutMetadata> {
    if (native && native.discoverLayoutsNative) {
        try {
            const raw = native.discoverLayoutsNative(layoutsDir)
            const layouts = new Map<string, LayoutMetadata>()
            for (const [name, metadata] of Object.entries(raw)) {
                // Adjust for Rust's camelCase vs Map mapping if needed
                const meta = metadata as any
                layouts.set(name, {
                    name: meta.name,
                    filePath: meta.filePath,
                    props: meta.props,
                    states: new Map(Object.entries(meta.states || {})),
                    html: meta.html,
                    scripts: meta.scripts,
                    styles: meta.styles
                })
            }
            return layouts
        } catch (error: any) {
            console.warn(`[Zenith Native] Layout discovery failed for ${layoutsDir}: ${error.message}`)
        }
    }

    return new Map<string, LayoutMetadata>()
}
