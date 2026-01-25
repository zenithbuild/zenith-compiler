import type { LayoutMetadata } from '../discovery/layouts'

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
 * Process a page by inlining a layout
 */
export function processLayout(
    source: string,
    layout: LayoutMetadata,
    props: Record<string, any> = {}
): string {
    if (native && native.processLayoutNative) {
        try {
            // Convert Map to record for native serialization
            const layoutForNative = {
                name: layout.name,
                html: layout.html,
                scripts: layout.scripts,
                styles: layout.styles
            };

            return native.processLayoutNative(
                source,
                JSON.stringify(layoutForNative),
                JSON.stringify(props)
            );
        } catch (error: any) {
            console.warn(`[Zenith Native] Layout processing failed: ${error.message}`);
        }
    }

    // Fallback: This should ideally not be reached if native is available
    return source;
}
