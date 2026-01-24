import { compileZenSource } from '../index';
import { InvariantError } from '../errors/compilerError';

/**
 * Integration Test: Native Error Bridge
 * Triggers an INV005 (TEMPLATE_TAG) error and verifies detailed info.
 */
async function testNativeErrorBridge() {
    console.log('Testing Native Error Bridge (INV001-INV006)...');

    const source = `
<script>
    const name = "Zenith";
</script>

<template>
    <div>This is invalid because of the template tag</div>
</template>
    `;

    try {
        await compileZenSource(source, 'test-invalid.zen');
        throw new Error('Should have thrown an InvariantError for <template> tag');
    } catch (e) {
        if (e instanceof InvariantError) {
            console.log('Caught InvariantError:', e.code);

            if (e.code !== 'INV005') {
                throw new Error(`Expected error code INV005, but got ${e.code}`);
            }
            if (e.context !== '<template>') {
                throw new Error(`Expected context "<template>", but got "${e.context}"`);
            }
            if (!e.hints || e.hints.length === 0) {
                throw new Error('Expected hints to be populated');
            }
            if (!e.hints.some(h => h.includes('Zenith component') || h.includes('Card.Header'))) {
                throw new Error('Hints do not contain expected content');
            }

            console.log('✅ Native error bridge test passed');
        } else {
            console.error('Unexpected error type:', e);
            throw e;
        }
    }
}

// Run tests
if (import.meta.main) {
    testNativeErrorBridge().catch(e => {
        console.error('❌ Integration test failed:', e.message);
        process.exit(1);
    });
}
