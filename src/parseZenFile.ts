/**
 * Zenith File Parser (Native Bridge)
 * 
 * Delegates parsing of .zen files to the Rust native compiler.
 */

import { readFileSync } from 'fs'
import type { ZenIR, StyleIR } from './ir/types'
import { CompilerError } from './errors/compilerError'

let native: any
try {
    try {
        native = require('../native/compiler-native')
    } catch {
        native = require('../native/compiler-native/index.js')
    }
} catch (e) {
    // Bridge load handled elsewhere
}

/**
 * Parse a .zen file into IR via Native Bridge
 */
export function parseZenFile(filePath: string, sourceInput?: string): ZenIR {
    let source: string

    if (sourceInput) {
        source = sourceInput
    } else {
        try {
            source = readFileSync(filePath, 'utf-8')
        } catch (error: any) {
            throw new CompilerError(
                `Failed to read file: ${error.message}`,
                filePath,
                1,
                1
            )
        }
    }

    if (native && native.parseTemplateNative && native.parseScriptNative && native.extractStylesNative) {

        try {
            const template = native.parseTemplateNative(source, filePath)
            const script = native.parseScriptNative(source)
            const stylesRaw = native.extractStylesNative(source)
            const styles: StyleIR[] = stylesRaw.map((s: string) => ({ raw: s }));

            return {
                filePath,
                template,
                script,
                styles
            }
        } catch (error: any) {
            console.warn(`[Zenith Native] Parsing failed for ${filePath}: ${error.message}`)
            throw error
        }
    }

    throw new Error(`[Zenith Native] Parser bridge unavailable - cannot compile ${filePath}`)
}
