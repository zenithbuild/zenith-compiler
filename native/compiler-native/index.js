const { join } = require('path');
const { statSync } = require('fs');

let nativeBinding;

// Get the mtime of the .node file to bust bun's cache
const nodePath = join(__dirname, 'compiler-native.node');
const mtime = statSync(nodePath).mtimeMs;

try {
    // Clear require cache to force reload
    delete require.cache[require.resolve('./compiler-native.node')];
    nativeBinding = require('./compiler-native.node');
} catch (e) {
    console.error('[Zenith Native] Failed to load native binding:', e.message);
    throw e;
}

module.exports = nativeBinding;
