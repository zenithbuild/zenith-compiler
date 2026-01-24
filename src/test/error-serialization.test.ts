import { InvariantError, CompilerError } from '../errors/compilerError';

/**
 * Test cases for the enhanced Error System
 */
function testErrorStructure() {
    console.log('Testing CompilerError structure...');

    const error = new CompilerError(
        'Test message',
        'test.zen',
        10,
        5,
        'COMPILER_ERROR',
        'TEST_CONTEXT',
        ['Hint 1', 'Hint 2']
    );

    console.assert(error.message.includes('Test message'), 'Message should be preserved');
    console.assert(error.file === 'test.zen', 'File should be preserved');
    console.assert(error.line === 10, 'Line should be preserved');
    console.assert(error.column === 5, 'Column should be preserved');
    console.assert(error.context === 'TEST_CONTEXT', 'Context should be preserved');
    console.assert(error.hints.length === 2, 'Hints should be preserved');
    console.assert(error.hints[0] === 'Hint 1', 'Hint 1 should match');

    console.log('✅ CompilerError structure test passed');
}

function testInvariantErrorStructure() {
    console.log('Testing InvariantError structure...');

    const error = new InvariantError(
        'INV001',
        'Invariant failed',
        'Always be true',
        'test.zen',
        20,
        15,
        'INVARIANT_CONTEXT',
        ['Fix hint']
    );

    console.assert(error.code === 'INV001', 'Code should be preserved');
    console.assert(error.guarantee === 'Always be true', 'Guarantee should be preserved');
    console.assert(error.errorType === 'InvariantViolation', 'Default errorType should be InvariantViolation');
    console.assert(error.context === 'INVARIANT_CONTEXT', 'Context should be preserved');
    console.assert(error.hints[0] === 'Fix hint', 'Hints should be preserved');

    console.log('✅ InvariantError structure test passed');
}

// Run tests
if (import.meta.main) {
    try {
        testErrorStructure();
        testInvariantErrorStructure();
        console.log('✅ All Error System unit tests passed!');
    } catch (e) {
        console.error('❌ Tests failed:', e);
        process.exit(1);
    }
}
