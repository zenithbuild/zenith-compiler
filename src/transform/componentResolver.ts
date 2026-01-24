
import type { ZenIR } from '../ir/types'
import type { ComponentMetadata } from '../discovery/componentDiscovery'
import { resolveComponentsNative } from '../../native/compiler-native'

/**
 * Inline all components in a ZenIR.
 * 
 * This is Stage 2 of the compiler:
 * - Expands <Component /> tags into their templates.
 * - Renames local symbols in component scripts and expressions to avoid collisions.
 * - Merges component scripts directly into the main page script.
 * - Promotes component expressions to the page-level expression registry.
 * 
 * NATIVE IMPLEMENTATION:
 * Delegates to the Rust native compiler for performance and correctness.
 */
export function resolveComponentsInIR(
    ir: ZenIR,
    components: Map<string, ComponentMetadata>
): ZenIR {
    console.error(`[ZenithDebug] resolveComponentsInIR called with ${components.size} components`);
    for (const [name, meta] of components) {
        console.error(`[ZenithDebug] Component '${name}': script=${meta.script ? meta.script.length : 'null'} bytes`);
    }

    const irJson = JSON.stringify(ir);

    // Convert Map to record for JSON serialization
    const componentsRecord: Record<string, ComponentMetadata> = {};
    for (const [key, value] of components) {
        componentsRecord[key] = value;
    }
    const componentsJson = JSON.stringify(componentsRecord);

    console.error(`[ZenithDebug] Calling resolveComponentsNative...`);
    const resolvedJson = resolveComponentsNative(irJson, componentsJson);
    console.error(`[ZenithDebug] resolveComponentsNative returned ${resolvedJson.length} bytes`);

    return JSON.parse(resolvedJson) as ZenIR;
}
