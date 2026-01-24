# @zenith/compiler 🏗️

The Iron Heart of the Zenith framework. High-performance native compiler and build-time architect.

## Overview

@zenith/compiler owns everything related to structure, wiring, and validation. It is a coordinated companion to `@zenith/core`.

### Core Responsibilities
- **Parsing**: Native AST parsing of `.zen` files (Rust).
- **Transformation**: Lowering templates to optimized execution plans.
- **Dependency Management**: Compile-time resolution of imports and reactive graphs.
- **CSS Engine**: High-speed CSS compilation and Tailwind integration.
- **Bundle Generation**: Composing the thin client runtime for the browser.

## Coordinated System

Zenith is built as a coordinated system. The compiler produces artifacts that the Core runtime consumes blindly.
- **No runtime decisions**: If it can be known at compile time, the compiler decides it.
- **Tight Coupling**: Versioned and released in lockstep with `@zenith/core`.

## Internal Structure

- `native/`: The Rust-powered core compiler.
- `src/parse/`: TypeScript wrappers for the native parser.
- `src/runtime/`: logic for generating the `bundle.js` target.

## License

MIT
