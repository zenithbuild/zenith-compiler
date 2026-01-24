/**
 * Zenith Client Runtime - Final Canonical Implementation
 * Satisfies strict reactivity requirements:
 * - Signals are functions (get/set)
 * - State is a Proxy
 * - Effects support cleanup/debounce
 * - Memos are computed signals
 * - Direct DOM updates via fine-grained reactivity
 */

let currentEffect: any = null;
const contextStack: any[] = [];
let batchDepth = 0;
const pendingEffects = new Set<any>();

function pushContext(effect: any) {
    contextStack.push(currentEffect);
    currentEffect = effect;
}

function popContext() {
    currentEffect = contextStack.pop();
}

function trackDependency(subscribers: Set<any>) {
    if (currentEffect) {
        subscribers.add(currentEffect);
        currentEffect.dependencies.add(subscribers);
    }
}

function notifySubscribers(subscribers: Set<any>) {
    const effects = Array.from(subscribers);
    for (const effect of effects) {
        if (batchDepth > 0) {
            pendingEffects.add(effect);
        } else {
            effect.run();
        }
    }
}

function cleanupEffect(effect: any) {
    for (const dependency of effect.dependencies) {
        dependency.delete(effect);
    }
    effect.dependencies.clear();
}

// === zenSignal ===
export function zenSignal<T>(initialValue: T) {
    let value = initialValue;
    const subscribers = new Set<any>();

    function signal(newValue?: T) {
        if (arguments.length === 0) {
            trackDependency(subscribers);
            return value;
        }
        if (newValue !== value) {
            value = newValue as T;
            notifySubscribers(subscribers);
        }
        return value;
    }

    signal._isSignal = true;
    signal.toString = () => String(value);
    signal.valueOf = () => value;
    // .value getter for convenience/compat
    Object.defineProperty(signal, 'value', {
        get: () => signal(),
        set: (v) => signal(v)
    });

    return signal;
}

// === zenState ===
export function zenState<T extends object>(initialObj: T): T {
    const subscribers = new Map<string, Set<any>>();

    function getSubscribers(path: string): Set<any> {
        if (!subscribers.has(path)) {
            subscribers.set(path, new Set());
        }
        return subscribers.get(path)!;
    }

    function createProxy(obj: any, parentPath: string = ''): any {
        if (obj === null || typeof obj !== 'object' || obj._isSignal) return obj;

        return new Proxy(obj, {
            get(target, prop) {
                if (typeof prop === 'symbol') return target[prop];
                const path = parentPath ? `${parentPath}.${String(prop)}` : String(prop);
                trackDependency(getSubscribers(path));

                const value = target[prop];
                if (value !== null && typeof value === 'object' && !value._isSignal) {
                    return createProxy(value, path);
                }
                return value;
            },
            set(target, prop, newValue) {
                if (typeof prop === 'symbol') { target[prop] = newValue; return true; }
                const path = parentPath ? `${parentPath}.${String(prop)}` : String(prop);
                const oldValue = target[prop];

                if (oldValue && typeof oldValue === 'function' && oldValue._isSignal) {
                    oldValue(newValue);
                } else if (oldValue !== newValue) {
                    target[prop] = newValue;
                    notifySubscribers(getSubscribers(path));

                    // Bubble up
                    const parts = path.split('.');
                    for (let i = parts.length - 1; i >= 0; i--) {
                        const pp = parts.slice(0, i).join('.');
                        if (pp) notifySubscribers(getSubscribers(pp));
                    }
                }
                return true;
            }
        });
    }

    return createProxy(initialObj);
}

// === zenEffect ===
export function zenEffect(fn: () => void | (() => void), options: { debounce?: number, defer?: boolean } = {}) {
    let cleanup: any;
    let timeout: any;

    const effect = {
        dependencies: new Set<Set<any>>(),
        run: () => {
            if (options.debounce) {
                if (timeout) clearTimeout(timeout);
                timeout = setTimeout(execute, options.debounce);
            } else {
                execute();
            }
        }
    };

    function execute() {
        cleanupEffect(effect);
        pushContext(effect);
        try {
            if (cleanup) cleanup();
            cleanup = fn();
        } finally {
            popContext();
        }
    }

    if (!options.defer) execute();
    return () => {
        cleanupEffect(effect);
        if (cleanup) cleanup();
    };
}

// === zenMemo ===
export function zenMemo<T>(fn: () => T) {
    const signal = zenSignal<T>(undefined as any);
    zenEffect(() => { signal(fn()); });
    const memo = () => signal();
    memo._isSignal = true;
    return memo;
}

// === zenRef ===
export function zenRef<T>(initial?: T) {
    return { current: initial || null };
}

// === zenBatch ===
export function zenBatch(fn: () => void) {
    batchDepth++;
    try { fn(); } finally {
        batchDepth--;
        if (batchDepth === 0) {
            const effects = Array.from(pendingEffects);
            pendingEffects.clear();
            effects.forEach(e => e.run());
        }
    }
}

export function zenUntrack<T>(fn: () => T): T {
    pushContext(null);
    try { return fn(); } finally { popContext(); }
}

// === Component Instance & Lifecycles ===
export interface ComponentInstance {
    mountHooks: Array<() => void | (() => void)>;
}
let activeInstance: ComponentInstance | null = null;
export function setActiveInstance(inst: ComponentInstance | null) { activeInstance = inst; }

export function zenOnMount(cb: any) {
    if (activeInstance) activeInstance.mountHooks.push(cb);
    // Fallback: if no active instance (e.g. top level), run immediately? 
    // Better to strict scope it, but for safety in simple usages:
    else if (typeof window !== 'undefined' && document.readyState === 'complete') {
        // cb(); // Uncomment if lax behavior desired
    }
}
export function zenOnUnmount(cb: any) {
    // Placeholder: requires unmount hooks in instance
}

export function triggerMount(inst: ComponentInstance) {
    if (inst && inst.mountHooks) {
        inst.mountHooks.forEach(cb => {
            const cleanup = cb();
            // TODO: Store cleanup for unmount
        });
    }
}

// === Rendering & Hydration ===
const SVG_NAMESPACE = 'http://www.w3.org/2000/svg';
const SVG_TAGS = new Set([
    'svg', 'path', 'circle', 'ellipse', 'line', 'polyline', 'polygon', 'rect',
    'g', 'defs', 'use', 'symbol', 'clipPath', 'mask', 'pattern', 'marker',
    'linearGradient', 'radialGradient', 'stop', 'filter', 'feGaussianBlur',
    'feOffset', 'feBlend', 'feColorMatrix', 'feComponentTransfer', 'feComposite',
    'feConvolveMatrix', 'feDiffuseLighting', 'feDisplacementMap', 'feFlood',
    'feFuncA', 'feFuncB', 'feFuncG', 'feFuncR', 'feImage', 'feMerge', 'feMergeNode',
    'feMorphology', 'fePointLight', 'feSpotLight', 'feSpecularLighting', 'feTile',
    'feTurbulence', 'foreignObject', 'image', 'switch', 'text', 'tspan', 'textPath',
    'title', 'desc', 'metadata', 'a', 'view', 'animate', 'animateMotion',
    'animateTransform', 'set', 'mpath'
]);

// Track current namespace context for nested elements
let currentNamespace: string | null = null;

export function h(tag: string, props: any, ...children: any[]): Element {
    // Determine if this element should be in SVG namespace
    const isSvgTag = SVG_TAGS.has(tag) || SVG_TAGS.has(tag.toLowerCase());
    const useSvgNamespace = isSvgTag || currentNamespace === SVG_NAMESPACE;

    // Create element with correct namespace
    const el = useSvgNamespace
        ? document.createElementNS(SVG_NAMESPACE, tag)
        : document.createElement(tag);

    // Track namespace context for children
    const previousNamespace = currentNamespace;
    if (tag === 'svg' || tag === 'SVG') {
        currentNamespace = SVG_NAMESPACE;
    }

    if (props) {
        for (const [key, val] of Object.entries(props)) {
            if (key === 'ref') {
                if (val && typeof val === 'object' && 'current' in (val as any)) (val as any).current = el;
                else if (typeof val === 'string') {
                    const state = (window as any).__ZENITH_STATE__;
                    if (state && state[val] && typeof state[val] === 'object' && 'current' in state[val]) state[val].current = el;
                }
            } else if (key.startsWith('on') && typeof val === 'function') {
                el.addEventListener(key.slice(2).toLowerCase(), (e) => {
                    // Check if val returns a function (expression wrapper) or is the handler
                    const res = val(e, el);
                    if (typeof res === 'function') res(e, el);
                });
            } else if (typeof val === 'function') {
                zenEffect(() => { updateAttr(el, key, val()); });
            } else {
                updateAttr(el, key, val);
            }
        }
    }
    const spread = (items: any[]): any[] => items.reduce((acc, v) => acc.concat(Array.isArray(v) ? spread(v) : v), []);
    spread(children).forEach(child => {
        if (typeof child === 'function') {
            // For reactive expressions, we need a placeholder that can be updated
            // The placeholder is a comment node that marks where content should go
            const placeholder = document.createComment('expr');
            el.appendChild(placeholder);
            let currentNodes: Node[] = [];

            zenEffect(() => {
                const result = child();
                // Remove old nodes
                currentNodes.forEach(n => { if (n.parentNode) n.parentNode.removeChild(n); });
                currentNodes = [];

                if (result == null) {
                    // null/undefined - render nothing
                } else if (result instanceof Node) {
                    // DOM node - insert it
                    placeholder.before(result);
                    currentNodes = [result];
                } else if (Array.isArray(result)) {
                    // Array of nodes/strings
                    result.forEach(item => {
                        const node = item instanceof Node ? item : document.createTextNode(String(item));
                        placeholder.before(node);
                        currentNodes.push(node);
                    });
                } else {
                    // Primitive (string, number) - create text node
                    const textNode = document.createTextNode(String(result));
                    placeholder.before(textNode);
                    currentNodes = [textNode];
                }
            });
        } else {
            el.appendChild(child instanceof Node ? child : document.createTextNode(String(child)));
        }
    });

    // Restore previous namespace context
    currentNamespace = previousNamespace;

    return el;
}

function updateAttr(el: Element, key: string, val: any) {
    if (key === 'class' || key === 'className') {
        // SVG uses className.baseVal, HTML uses className directly
        if ('className' in el && typeof (el as any).className === 'object') {
            (el as SVGElement).className.baseVal = String(val || '');
        } else {
            (el as HTMLElement).className = String(val || '');
        }
    }
    else if (key === 'style' && typeof val === 'object') Object.assign((el as HTMLElement).style, val);
    else if (val == null || val === false) el.removeAttribute(key);
    else el.setAttribute(key, String(val));
}

export function fragment(children: any) {
    const frag = document.createDocumentFragment();
    const items = typeof children === 'function' ? children() : children;
    const spread = (its: any[]): any[] => its.reduce((acc, v) => acc.concat(Array.isArray(v) ? spread(v) : v), []);
    spread(Array.isArray(items) ? items : [items]).forEach(item => {
        frag.appendChild(item instanceof Node ? item : document.createTextNode(String(item)));
    });
    return frag;
}

export function zenLoop(sourceFn: () => any[], itemFn: (item: any, idx: number) => Node) {
    const start = document.createComment('loop:start');
    const end = document.createComment('loop:end');
    const frag = document.createDocumentFragment();
    frag.append(start, end);
    zenEffect(() => {
        const items = sourceFn() || [];
        let curr = start.nextSibling;
        while (curr && curr !== end) { const next = curr.nextSibling; curr.remove(); curr = next; }
        items.forEach((item, i) => end.before(itemFn(item, i)));
    });
    return frag;
}

export function zenithHydrate(state: any, container: Element | Document = document) {
    const ir = (window as any).canonicalIR;
    if (!ir) return;
    (window as any).__ZENITH_STATE__ = state;
    const nodes = ir(state);
    const mount = container === document ? document.body : (container as HTMLElement);
    mount.innerHTML = '';
    const arr = Array.isArray(nodes) ? nodes : [nodes];
    arr.forEach(n => {
        if (n == null || n === false) return;
        // If it's a full <html> node, we might need to be more careful, 
        // but for now let's just avoid stringifying nulls.
        mount.appendChild(n instanceof Node ? n : document.createTextNode(String(n)));
    });
}

// === Global Setup ===
export function setup() {
    if (typeof window === 'undefined') return;
    const w = window as any;
    w.__zenith = { h, fragment, loop: zenLoop, state: zenState, signal: zenSignal, effect: zenEffect, memo: zenMemo, ref: zenRef, batch: zenBatch, onMount: zenOnMount, setActiveInstance, triggerMount, spread_children: (a: any) => a };
    w.zenithHydrate = zenithHydrate;
    // Expose globals for imported usage
    w.zenSignal = zenSignal;
    w.zenState = zenState;
    w.zenEffect = zenEffect;
    w.zenMemo = zenMemo;
    w.zenRef = zenRef;
    w.zenBatch = zenBatch;
    w.zenUntrack = zenUntrack;
    w.zenOnMount = zenOnMount;
    // [ZENITH-NATIVE] zenOrder: Scheduling primitive
    w.zenOrder = (fn: any) => { if (typeof fn === 'function') fn(); };
}
setup();
