(function () {
    if (typeof window === 'undefined') return;
    let cE = null; const cS = []; let bD = 0; const pE = new Set();
    let isFlushing = false; let flushScheduled = false;
    function pC(e) { cS.push(cE); cE = e; }
    function oC() { cE = cS.pop(); }
    function tD(s) { if (cE) { s.add(cE); cE.dependencies.add(s); } }
    function nS(s) {
        const es = Array.from(s);
        for (const e of es) {
            // Don't queue an effect that's currently running
            if (e.isRunning) continue;
            if (bD > 0 || isFlushing) pE.add(e);
            else e.run();
        }
    }
    function scheduleFlush() {
        if (flushScheduled) return;
        flushScheduled = true;
        queueMicrotask(() => {
            flushScheduled = false;
            flushEffects();
        });
    }
    function flushEffects() {
        if (isFlushing || bD > 0) return;
        isFlushing = true;
        try {
            while (pE.size > 0) {
                const efs = Array.from(pE);
                pE.clear();
                for (const e of efs) {
                    if (!e.isRunning) e.run();
                }
            }
        } finally {
            isFlushing = false;
        }
    }
    function cEf(e) { for (const d of e.dependencies) d.delete(e); e.dependencies.clear(); }
    window.zenSignal = function (v) {
        const s = new Set();
        function sig(nV) {
            if (arguments.length === 0) { tD(s); return v; }
            if (nV !== v) { v = nV; nS(s); scheduleFlush(); }
            return v;
        }
        sig._isSignal = true; sig.toString = () => String(v); sig.valueOf = () => v;
        return sig;
    };
    window.zenState = function (o) {
        const subs = new Map();
        function gS(p) { if (!subs.has(p)) subs.set(p, new Set()); return subs.get(p); }
        function cP(obj, pPath = '') {
            if (obj === null || typeof obj !== 'object' || obj._isSignal) return obj;
            return new Proxy(obj, {
                get(t, p) {
                    if (typeof p === 'symbol') return t[p];
                    const path = pPath ? `${pPath}.${String(p)}` : String(p);
                    tD(gS(path));
                    const v = t[p];
                    if (v !== null && typeof v === 'object' && !v._isSignal) return cP(v, path);
                    return v;
                },
                set(t, p, nV) {
                    if (typeof p === 'symbol') { t[p] = nV; return true; }
                    const path = pPath ? `${pPath}.${String(p)}` : String(p);
                    const oV = t[p];
                    if (oV && typeof oV === 'function' && oV._isSignal) oV(nV);
                    else if (oV !== nV) {
                        t[p] = nV; nS(gS(path));
                        const pts = path.split('.');
                        for (let i = pts.length - 1; i >= 0; i--) {
                            const pp = pts.slice(0, i).join('.');
                            if (pp) nS(gS(pp));
                        }
                        scheduleFlush();
                    }
                    return true;
                }
            });
        }
        return cP(o);
    };
    // .zen Template Bindings:
    // {count} automatically unwraps signals (zenSignal) or state props (zenState)
    // This allows direct usage in templates without calling functions (no {count()})
    // The runtime tracks dependencies for fine-grained DOM updates via zenEffect.
    window.zenEffect = function (fn, opts = {}) {
        let cl, tm;
        const ef = {
            dependencies: new Set(),
            isRunning: false,
            run: () => {
                if (ef.isRunning) return; // Re-entrancy guard
                const schedule = opts.scheduler || (f => f());
                if (opts.debounce) {
                    if (tm) clearTimeout(tm);
                    tm = setTimeout(() => schedule(ex), opts.debounce);
                } else schedule(ex);
            }
        };
        function ex() {
            if (ef.isRunning) return; // Double-check re-entrancy
            ef.isRunning = true;
            cEf(ef);
            pC(ef);
            try { if (cl) cl(); cl = fn(); }
            finally {
                oC();
                ef.isRunning = false;
                // Don't call flushEffects here - let the microtask handle it
            }
        }
        if (!opts.defer) ex();
        return () => { cEf(ef); if (cl) cl(); };
    };

    window.zenMemo = function (fn) {
        const sig = window.zenSignal();
        window.zenEffect(() => sig(fn()));
        const m = () => sig(); m._isSignal = true; return m;
    };
    window.zenBatch = function (fn) {
        bD++;
        try { fn(); } finally {
            bD--;
            if (bD === 0) flushEffects();
        }
    };
    window.zenUntrack = function (fn) {
        pC(null);
        try { return fn(); } finally { oC(); }
    };

    window.zenRef = (i) => ({ current: i || null });
    window.zenOnMount = (cb) => { if (window.__zenith && window.__zenith.activeInstance) window.__zenith.activeInstance.mountHooks.push(cb); };
    window.zenOnUnmount = (cb) => { /* TODO: implement unmount hooks */ };
    window.zenithHydrate = function (state, container = document) {
        const ir = window.canonicalIR; if (!ir) return;
        window.__ZENITH_STATE__ = state;
        const nodes = ir(state);
        const mount = container === document ? document.body : container;
        mount.innerHTML = '';
        (Array.isArray(nodes) ? nodes : [nodes]).forEach(n => mount.appendChild(n instanceof Node ? n : document.createTextNode(String(n))));
    };
    /* [ZENITH-NATIVE] zenOrder: Scheduling primitive for ordered effects/animations */
    window.zenOrder = function (fn) {
        if (typeof fn === 'function') fn();
    };
    // Track current SVG namespace context for nested elements
    let currentNamespace = null;

    window.__zenith = {
        h: function (tag, props, children) {
            // SVG elements must be created with the SVG namespace
            const SVG_NS = 'http://www.w3.org/2000/svg';
            const SVG_TAGS = new Set(['svg', 'path', 'circle', 'ellipse', 'line', 'polygon', 'polyline', 'rect', 'g', 'defs', 'clipPath', 'mask', 'use', 'symbol', 'text', 'tspan', 'textPath', 'image', 'foreignObject', 'switch', 'desc', 'title', 'metadata', 'linearGradient', 'radialGradient', 'stop', 'pattern', 'filter', 'feBlend', 'feColorMatrix', 'feComponentTransfer', 'feComposite', 'feConvolveMatrix', 'feDiffuseLighting', 'feDisplacementMap', 'feFlood', 'feGaussianBlur', 'feImage', 'feMerge', 'feMergeNode', 'feMorphology', 'feOffset', 'feSpecularLighting', 'feTile', 'feTurbulence', 'animate', 'animateMotion', 'animateTransform', 'set', 'marker']);

            // Determine if this element should be in SVG namespace
            const isSvgTag = SVG_TAGS.has(tag) || SVG_TAGS.has(tag.toLowerCase());
            const useSvgNamespace = isSvgTag || currentNamespace === SVG_NS;

            // Create element with correct namespace
            const el = useSvgNamespace ? document.createElementNS(SVG_NS, tag) : document.createElement(tag);

            // Track namespace context for children (save previous, set new if svg root)
            const previousNamespace = currentNamespace;
            if (tag === 'svg' || tag === 'SVG') {
                currentNamespace = SVG_NS;
            }
            if (props) {
                // Helper to set class for both HTML and SVG elements
                const setClass = (element, value) => {
                    if (useSvgNamespace && 'className' in element && typeof element.className === 'object') {
                        element.className.baseVal = String(value || '');
                    } else {
                        element.className = String(value || '');
                    }
                };

                for (const [k, v] of Object.entries(props)) {
                    if (k === 'ref') {
                        if (v && typeof v === 'object' && 'current' in v) v.current = el;
                        else if (typeof v === 'string') {
                            const s = window.__ZENITH_STATE__;
                            if (s && s[v] && typeof s[v] === 'object' && 'current' in s[v]) s[v].current = el;
                        }
                    } else if (k.startsWith('on') && typeof v === 'function') {
                        el.addEventListener(k.slice(2).toLowerCase(), (e) => {
                            const h = v(e, el); if (typeof h === 'function') h(e, el);
                        });
                    } else if (typeof v === 'function') {
                        window.zenEffect(() => {
                            const val = v();
                            if (k === 'class' || k === 'className') setClass(el, val);
                            else if (val == null || val === false) el.removeAttribute(k);
                            else if (el.setAttribute) el.setAttribute(k, String(val));
                        });
                    } else {
                        if (k === 'class' || k === 'className') setClass(el, v);
                        else if (el.setAttribute) el.setAttribute(k, String(v));
                    }
                }
            }
            const spr = (its) => its.reduce((acc, v) => acc.concat(Array.isArray(v) ? spr(v) : v), []);
            spr(children || []).forEach(c => {
                if (c == null || c === false) return; // Skip null/undefined/false
                if (typeof c === 'function') {
                    // Reactive child - use placeholder and update dynamically
                    const ph = document.createComment('expr');
                    el.appendChild(ph);
                    let curNodes = [];
                    window.zenEffect(() => {
                        const r = c();
                        // Remove old nodes
                        curNodes.forEach(n => { if (n.parentNode) n.parentNode.removeChild(n); });
                        curNodes = [];
                        if (r == null || r === false) {
                            // Render nothing
                        } else if (r instanceof Node) {
                            ph.parentNode.insertBefore(r, ph);
                            curNodes = [r];
                        } else if (Array.isArray(r)) {
                            r.flat(Infinity).forEach(item => {
                                if (item == null || item === false) return;
                                const node = item instanceof Node ? item : document.createTextNode(String(item));
                                ph.parentNode.insertBefore(node, ph);
                                curNodes.push(node);
                            });
                        } else {
                            const tn = document.createTextNode(String(r));
                            ph.parentNode.insertBefore(tn, ph);
                            curNodes = [tn];
                        }
                    });
                } else {
                    el.appendChild(c instanceof Node ? c : document.createTextNode(String(c)));
                }
            });

            // Restore previous namespace context after processing children
            currentNamespace = previousNamespace;

            return el;
        },
        fragment: function (c) {
            const f = document.createDocumentFragment();
            const spr = (its) => its.reduce((acc, v) => acc.concat(Array.isArray(v) ? spr(v) : v), []);
            spr(Array.isArray(c) ? c : [c]).forEach(i => f.appendChild(i instanceof Node ? i : document.createTextNode(String(i))));
            return f;
        },
        triggerMount: (inst) => { if (inst && inst.mountHooks) inst.mountHooks.forEach(cb => cb()); },
        setActiveInstance: (inst) => { if (window.__zenith) window.__zenith.activeInstance = inst; }
    };

    /**
     * Fix SVG namespace for elements inserted via innerHTML.
     * When HTML containing SVG is inserted via innerHTML, browsers create
     * SVG elements in XHTML namespace instead of SVG namespace. This breaks
     * GSAP DrawSVGPlugin and other SVG-specific functionality.
     * 
     * This function re-parses the SVG using DOMParser with the correct
     * MIME type and replaces the malformed SVG with a correctly namespaced one.
     * 
     * @param {string} selector - CSS selector for the SVG element(s) to fix
     */
    window.zenFixSVGNamespace = function (selector) {
        const svgs = document.querySelectorAll(selector);
        svgs.forEach(svg => {
            // Skip if already in correct namespace
            if (svg.namespaceURI === 'http://www.w3.org/2000/svg') return;

            // Get the outer HTML and re-parse with correct MIME type
            const svgString = svg.outerHTML;
            const parser = new DOMParser();
            const doc = parser.parseFromString(svgString, 'image/svg+xml');
            const newSvg = doc.documentElement;

            // Check for parse errors
            if (newSvg.tagName === 'parsererror' || doc.querySelector('parsererror')) {
                console.warn('[Zenith] Failed to fix SVG namespace:', selector);
                return;
            }

            // Copy over any inline styles that might have been applied
            if (svg.style.cssText) {
                newSvg.style.cssText = svg.style.cssText;
            }

            // Replace the old SVG with the correctly namespaced one
            svg.parentNode.replaceChild(document.importNode(newSvg, true), svg);
        });
    };
})();
