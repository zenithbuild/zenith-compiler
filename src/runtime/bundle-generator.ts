import { readFileSync, existsSync } from 'fs'
import path from 'path'

/**
 * Zenith Bundle Generator
 * 
 * Generates the shared client runtime bundle that gets served as:
 * - /assets/bundle.js in production
 * - /runtime.js in development
 * 
 * This is a cacheable, versioned file that contains:
 * - Reactivity primitives (zenSignal, zenState, zenEffect, etc.)
 * - Lifecycle hooks (zenOnMount, zenOnUnmount)
 * - Hydration functions (zenithHydrate)
 * - Event binding utilities
 */

/**
 * Generate the complete client runtime bundle
 * This is served as an external JS file, not inlined
 */
export function generateBundleJS(pluginData?: Record<string, any>): string {
  // Serialize plugin data blindly - CLI never inspects what's inside.
  // We escape </script> sequences just in case this bundle is ever inlined (unlikely but safe).
  const serializedData = pluginData
    ? JSON.stringify(pluginData).replace(/<\/script/g, '<\\/script')
    : '{}';

  // Resolve core runtime paths - assumes sibling directory or relative in node_modules
  const rootDir = process.cwd()
  const coreDistPath = path.join(rootDir, '../zenith-core/dist/runtime')

  let reactivityJS = ''
  let lifecycleJS = ''

  try {
    const reactivityFile = path.join(coreDistPath, 'reactivity/index.js')
    const lifecycleFile = path.join(coreDistPath, 'lifecycle/index.js')

    if (existsSync(reactivityFile)) reactivityJS = readFileSync(reactivityFile, 'utf-8')
    if (existsSync(lifecycleFile)) lifecycleJS = readFileSync(lifecycleFile, 'utf-8')
  } catch (e) {
    console.warn('[Zenith] Could not load runtime from core dist, falling back to basic support', e)
  }

  return `/*!
 * Zenith Runtime v1.0.1
 * Shared client-side runtime for hydration and reactivity
 */
(function(global) {
  'use strict';
  
  // Initialize plugin data envelope
  global.__ZENITH_PLUGIN_DATA__ = ${serializedData};
  
${reactivityJS ? `  // ============================================
  // Core Reactivity (Injected from @zenithbuild/core)
  // ============================================
  ${reactivityJS}` : `  // Fallback: Reactivity not found`}

${lifecycleJS ? `  // ============================================
  // Lifecycle Hooks (Injected from @zenithbuild/core)
  // ============================================
  ${lifecycleJS}` : `  // Fallback: Lifecycle not found`}
  
  // ============================================
  // Component Instance System
  // ============================================
  // Each component instance gets isolated state, effects, and lifecycles
  // Instances are tied to DOM elements via hydration markers
  
  const componentRegistry = {};
  
  function createComponentInstance(componentName, rootElement) {
    const instanceMountCallbacks = [];
    const instanceUnmountCallbacks = [];
    const instanceEffects = [];
    let instanceMounted = false;
    
    return {
      // DOM reference
      root: rootElement,
      
      // Lifecycle hooks (instance-scoped)
      onMount: function(fn) {
        if (instanceMounted) {
          const cleanup = fn();
          if (typeof cleanup === 'function') {
            instanceUnmountCallbacks.push(cleanup);
          }
        } else {
          instanceMountCallbacks.push(fn);
        }
      },
      onUnmount: function(fn) {
        instanceUnmountCallbacks.push(fn);
      },
      
      // Reactivity (uses global primitives but tracks for cleanup)
      signal: function(initial) {
        return zenSignal(initial);
      },
      state: function(initial) {
        return zenState(initial);
      },
      ref: function(initial) {
        return zenRef(initial);
      },
      effect: function(fn) {
        const cleanup = zenEffect(fn);
        instanceEffects.push(cleanup);
        return cleanup;
      },
      memo: function(fn) {
        return zenMemo(fn);
      },
      batch: function(fn) {
        zenBatch(fn);
      },
      untrack: function(fn) {
        return zenUntrack(fn);
      },
      
      // Lifecycle execution
      mount: function() {
        instanceMounted = true;
        for (let i = 0; i < instanceMountCallbacks.length; i++) {
          try {
            const cleanup = instanceMountCallbacks[i]();
            if (typeof cleanup === 'function') {
              instanceUnmountCallbacks.push(cleanup);
            }
          } catch(e) {
            console.error('[Zenith] Component mount error:', componentName, e);
          }
        }
        instanceMountCallbacks.length = 0;
      },
      unmount: function() {
        instanceMounted = false;
        // Cleanup effects
        for (let i = 0; i < instanceEffects.length; i++) {
          try { 
            if (typeof instanceEffects[i] === 'function') instanceEffects[i](); 
          } catch(e) { 
            console.error('[Zenith] Effect cleanup error:', e); 
          }
        }
        instanceEffects.length = 0;
        // Run unmount callbacks
        for (let i = 0; i < instanceUnmountCallbacks.length; i++) {
          try { instanceUnmountCallbacks[i](); } catch(e) { console.error('[Zenith] Unmount error:', e); }
        }
        instanceUnmountCallbacks.length = 0;
      }
    };
  }
  
  function defineComponent(name, factory) {
    componentRegistry[name] = factory;
  }
  
  function instantiateComponent(name, props, rootElement) {
    const factory = componentRegistry[name];
    if (!factory) {
      if (name === 'ErrorPage') {
        // Built-in fallback for ErrorPage if not registered by user
        return fallbackErrorPage(props, rootElement);
      }
      console.warn('[Zenith] Component not found:', name);
      return null;
    }
    return factory(props, rootElement);
  }

  function renderErrorPage(error, metadata) {
    console.error('[Zenith Runtime Error]', error, metadata);
    
    // In production, we might want a simpler page, but for now let's use the high-fidelity one
    // if it's available.
    const container = document.getElementById('app') || document.body;
    
    // If we've already rendered an error page, don't do it again to avoid infinite loops
    if (window.__ZENITH_ERROR_RENDERED__) return;
    window.__ZENITH_ERROR_RENDERED__ = true;

    const errorProps = {
      message: error.message || 'Unknown Error',
      stack: error.stack,
      file: metadata.file || (error.file),
      line: metadata.line || (error.line),
      column: metadata.column || (error.column),
      errorType: metadata.errorType || error.name || 'RuntimeError',
      code: metadata.code || 'ERR500',
      context: metadata.context || (metadata.expressionId ? 'Expression: ' + metadata.expressionId : ''),
      hints: metadata.hints || [],
      isProd: false // Check env here if possible
    };

    // Try to instantiate the user's ErrorPage
    const instance = instantiateComponent('ErrorPage', errorProps, container);
    if (instance) {
      container.innerHTML = '';
      instance.mount();
    } else {
      // Fallback to basic HTML if ErrorPage component fails or is missing
      container.innerHTML = \`
        <div style="padding: 4rem; font-family: system-ui, sans-serif; background: #000; color: #fff; min-h: 100vh;">
          <h1 style="font-size: 3rem; margin-bottom: 1rem; color: #ef4444;">Zenith Runtime Error</h1>
          <p style="font-size: 1.5rem; opacity: 0.8;">\${errorProps.message}</p>
          <pre style="margin-top: 2rem; padding: 1rem; background: #111; border-radius: 8px; overflow: auto; font-size: 0.8rem; color: #888;">\${errorProps.stack}</pre>
        </div>
      \`;
    }
  }

  function fallbackErrorPage(props, el) {
    // This could be a more complex fallback, but for now we just return null 
    // to trigger the basic HTML fallback in renderErrorPage.
    return null;
  }
  
  /**
   * Hydrate components by discovering data-zen-component markers
   * This is the ONLY place component instantiation should happen
   */
  function hydrateComponents(container) {
    try {
      const componentElements = container.querySelectorAll('[data-zen-component]');
      
      for (let i = 0; i < componentElements.length; i++) {
        const el = componentElements[i];
        const componentName = el.getAttribute('data-zen-component');
        
        // Skip if already hydrated OR if handled by instance script (data-zen-inst)
        if (el.__zenith_instance || el.hasAttribute('data-zen-inst')) continue;
        
        // Parse props from data attribute if present
        const propsJson = el.getAttribute('data-zen-props') || '{}';
        let props = {};
        try {
          props = JSON.parse(propsJson);
        } catch(e) {
          console.warn('[Zenith] Invalid props JSON for', componentName);
        }
        
        try {
          // Instantiate component and bind to DOM element
          const instance = instantiateComponent(componentName, props, el);
          
          if (instance) {
            el.__zenith_instance = instance;
          }
        } catch (e) {
          renderErrorPage(e, { component: componentName, props: props });
        }
      }
    } catch (e) {
      renderErrorPage(e, { activity: 'hydrateComponents' });
    }
  }
  
  // ============================================
  // Expression Registry & Hydration
  // ============================================
  
  const expressionRegistry = new Map();
  
  function registerExpression(id, fn) {
    expressionRegistry.set(id, fn);
  }
  
  function getExpression(id) {
    return expressionRegistry.get(id);
  }
  
  function updateNode(node, exprId, pageState) {
    const expr = getExpression(exprId);
    if (!expr) return;
    
    zenEffect(function() {
      try {
        const result = expr(pageState);
        
        if (node.hasAttribute('data-zen-text')) {
          // Handle complex text/children results
          if (result === null || result === undefined || result === false) {
            node.textContent = '';
          } else if (typeof result === 'string') {
            if (result.trim().startsWith('<') && result.trim().endsWith('>')) {
              node.innerHTML = result;
            } else {
              node.textContent = result;
            }
          } else if (result instanceof Node) {
            node.innerHTML = '';
            node.appendChild(result);
          } else if (Array.isArray(result)) {
            node.innerHTML = '';
            const fragment = document.createDocumentFragment();
            result.flat(Infinity).forEach(item => {
              if (item instanceof Node) fragment.appendChild(item);
              else if (item != null && item !== false) fragment.appendChild(document.createTextNode(String(item)));
            });
            node.appendChild(fragment);
          } else {
            node.textContent = String(result);
          }
        } else {
          // Attribute update
          const attrNames = ['class', 'style', 'src', 'href', 'disabled', 'checked'];
          for (const attr of attrNames) {
            if (node.hasAttribute('data-zen-attr-' + attr)) {
              if (attr === 'class' || attr === 'className') {
                node.className = String(result || '');
              } else if (attr === 'disabled' || attr === 'checked') {
                if (result) node.setAttribute(attr, '');
                else node.removeAttribute(attr);
              } else {
                if (result != null && result !== false) node.setAttribute(attr, String(result));
                else node.removeAttribute(attr);
              }
            }
          }
        }
      } catch (e) {
        renderErrorPage(e, { expressionId: exprId, node: node });
      }
    });
  }

  /**
   * Hydrate a page with reactive bindings
   * Called after page HTML is in DOM
   */
  function updateLoopBinding(template, exprId, pageState) {
    const expr = getExpression(exprId);
    if (!expr) return;

    const itemVar = template.getAttribute('data-zen-item');
    const indexVar = template.getAttribute('data-zen-index');

    // Use a marker or a container next to the template to hold instances
    let container = template.__zen_container;
    if (!container) {
      container = document.createElement('div');
      container.style.display = 'contents';
      template.parentNode.insertBefore(container, template.nextSibling);
      template.__zen_container = container;
    }

    zenEffect(function() {
      try {
        const items = expr(pageState);
        if (!Array.isArray(items)) return;

        // Simple reconciliation: clear and redraw
        container.innerHTML = '';

        items.forEach(function(item, index) {
          const fragment = template.content.cloneNode(true);
          
          // Create child scope
          const childState = Object.assign({}, pageState);
          if (itemVar) childState[itemVar] = item;
          if (indexVar) childState[indexVar] = index;

          // Recursive hydration for the fragment
          zenithHydrate(childState, fragment);
          
          container.appendChild(fragment);
        });
      } catch (e) {
        renderErrorPage(e, { expressionId: exprId, activity: 'loopReconciliation' });
      }
    });
  }

  /**
   * Hydrate static HTML with dynamic expressions
   */
  function zenithHydrate(pageState, container) {
    try {
      container = container || document;
      
      // Find all text expression placeholders
      const textNodes = container.querySelectorAll('[data-zen-text]');
      textNodes.forEach(el => updateNode(el, el.getAttribute('data-zen-text'), pageState));
      
      // Find all attribute expression placeholders
      const attrNodes = container.querySelectorAll('[data-zen-attr-class], [data-zen-attr-style], [data-zen-attr-src], [data-zen-attr-href]');
      attrNodes.forEach(el => {
        const attrMatch = Array.from(el.attributes).find(a => a.name.startsWith('data-zen-attr-'));
        if (attrMatch) updateNode(el, attrMatch.value, pageState);
      });

      // Find all loop placeholders
      const loopNodes = container.querySelectorAll('template[data-zen-loop]');
      loopNodes.forEach(el => updateLoopBinding(el, el.getAttribute('data-zen-loop'), pageState));
      
      // Wire up event handlers
      const eventTypes = ['click', 'change', 'input', 'submit', 'focus', 'blur', 'keyup', 'keydown'];
      eventTypes.forEach(eventType => {
        const elements = container.querySelectorAll('[data-zen-' + eventType + ']');
        elements.forEach(el => {
          const handlerName = el.getAttribute('data-zen-' + eventType);
          if (handlerName && (global[handlerName] || getExpression(handlerName))) {
            el.addEventListener(eventType, function(e) {
              const handler = global[handlerName] || getExpression(handlerName);
              if (typeof handler === 'function') handler(e, el);
            });
          }
        });
      });
      
      // Trigger mount (only if container is root)
      if (container === document || container.id === 'app' || container.tagName === 'BODY') {
        triggerMount();
      }
    } catch (e) {
      renderErrorPage(e, { activity: 'zenithHydrate' });
    }
  }
  
  // ============================================
  // zenith:content - Content Engine
  // ============================================

  const schemaRegistry = new Map();
  const builtInEnhancers = {
    readTime: (item) => {
      const wordsPerMinute = 200;
      const text = item.content || '';
      const wordCount = text.split(/\\s+/).length;
      const minutes = Math.ceil(wordCount / wordsPerMinute);
      return Object.assign({}, item, { readTime: minutes + ' min' });
    },
    wordCount: (item) => {
      const text = item.content || '';
      const wordCount = text.split(/\\s+/).length;
      return Object.assign({}, item, { wordCount: wordCount });
    }
  };

  async function applyEnhancers(item, enhancers) {
    let enrichedItem = Object.assign({}, item);
    for (const enhancer of enhancers) {
      if (typeof enhancer === 'string') {
        const fn = builtInEnhancers[enhancer];
        if (fn) enrichedItem = await fn(enrichedItem);
      } else if (typeof enhancer === 'function') {
        enrichedItem = await enhancer(enrichedItem);
      }
    }
    return enrichedItem;
  }

  class ZenCollection {
    constructor(items) {
      this.items = [...items];
      this.filters = [];
      this.sortField = null;
      this.sortOrder = 'desc';
      this.limitCount = null;
      this.selectedFields = null;
      this.enhancers = [];
      this._groupByFolder = false;
    }
    where(fn) { this.filters.push(fn); return this; }
    sortBy(field, order = 'desc') { this.sortField = field; this.sortOrder = order; return this; }
    limit(n) { this.limitCount = n; return this; }
    fields(f) { this.selectedFields = f; return this; }
    enhanceWith(e) { this.enhancers.push(e); return this; }
    groupByFolder() { this._groupByFolder = true; return this; }
    get() {
      let results = [...this.items];
      for (const filter of this.filters) results = results.filter(filter);
      if (this.sortField) {
        results.sort((a, b) => {
          const valA = a[this.sortField];
          const valB = b[this.sortField];
          if (valA < valB) return this.sortOrder === 'asc' ? -1 : 1;
          if (valA > valB) return this.sortOrder === 'asc' ? 1 : -1;
          return 0;
        });
      }
      if (this.limitCount !== null) results = results.slice(0, this.limitCount);
      
      // Apply enhancers synchronously if possible
      if (this.enhancers.length > 0) {
        results = results.map(item => {
          let enrichedItem = Object.assign({}, item);
          for (const enhancer of this.enhancers) {
            if (typeof enhancer === 'string') {
              const fn = builtInEnhancers[enhancer];
              if (fn) enrichedItem = fn(enrichedItem);
            } else if (typeof enhancer === 'function') {
              enrichedItem = enhancer(enrichedItem);
            }
          }
          return enrichedItem;
        });
      }
      
      if (this.selectedFields) {
        results = results.map(item => {
          const newItem = {};
          this.selectedFields.forEach(f => { newItem[f] = item[f]; });
          return newItem;
        });
      }
      
      // Group by folder if requested
      if (this._groupByFolder) {
        const groups = {};
        const groupOrder = [];
        for (const item of results) {
          // Extract folder from slug (e.g., "getting-started/installation" -> "getting-started")
          const slug = item.slug || item.id || '';
          const parts = slug.split('/');
          const folder = parts.length > 1 ? parts[0] : 'root';
          
          if (!groups[folder]) {
            groups[folder] = {
              id: folder,
              title: folder.split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' '),
              items: []
            };
            groupOrder.push(folder);
          }
          groups[folder].items.push(item);
        }
        return groupOrder.map(f => groups[f]);
      }
      
      return results;
    }
  }

  function defineSchema(name, schema) { schemaRegistry.set(name, schema); }

  function zenCollection(collectionName) {
    // Access plugin data from the neutral envelope
    // Content plugin stores all items under 'content' namespace
    const pluginData = global.__ZENITH_PLUGIN_DATA__ || {};
    const contentItems = pluginData.content || [];
    
    // Filter by collection name (plugin owns data structure, runtime just filters)
    const data = Array.isArray(contentItems)
      ? contentItems.filter(item => item && item.collection === collectionName)
      : [];
    
    return new ZenCollection(data);
  }

  // ============================================
  // useZenOrder - Documentation ordering & navigation
  // ============================================
  
  function slugify(text) {
    return String(text || '')
      .toLowerCase()
      .replace(/[^\\w\\s-]/g, '')
      .replace(/\\s+/g, '-')
      .replace(/-+/g, '-')
      .trim();
  }
  
  function getDocSlug(doc) {
    const slugOrId = String(doc.slug || doc.id || '');
    const parts = slugOrId.split('/');
    const filename = parts[parts.length - 1];
    return filename ? slugify(filename) : slugify(doc.title || 'untitled');
  }
  
  function processRawSections(rawSections) {
    const sections = (rawSections || []).map(function(rawSection) {
      const sectionSlug = slugify(rawSection.title || rawSection.id || 'section');
      const items = (rawSection.items || []).map(function(item) {
        return Object.assign({}, item, {
          slug: getDocSlug(item),
          sectionSlug: sectionSlug,
          isIntro: item.intro === true || (item.tags && item.tags.includes && item.tags.includes('intro'))
        });
      });
      
      // Sort items: intro first, then order, then alphabetical
      items.sort(function(a, b) {
        if (a.isIntro && !b.isIntro) return -1;
        if (!a.isIntro && b.isIntro) return 1;
        if (a.order !== undefined && b.order !== undefined) return a.order - b.order;
        if (a.order !== undefined) return -1;
        if (b.order !== undefined) return 1;
        return (a.title || '').localeCompare(b.title || '');
      });
      
      return {
        id: rawSection.id || sectionSlug,
        title: rawSection.title || 'Untitled',
        slug: sectionSlug,
        order: rawSection.order !== undefined ? rawSection.order : (rawSection.meta && rawSection.meta.order),
        hasIntro: items.some(function(i) { return i.isIntro; }),
        items: items
      };
    });
    
    // Sort sections: order → hasIntro → alphabetical
    sections.sort(function(a, b) {
      if (a.order !== undefined && b.order !== undefined) return a.order - b.order;
      if (a.order !== undefined) return -1;
      if (b.order !== undefined) return 1;
      if (a.hasIntro && !b.hasIntro) return -1;
      if (!a.hasIntro && b.hasIntro) return 1;
      return a.title.localeCompare(b.title);
    });
    
    return sections;
  }
  
  function createZenOrder(rawSections) {
    const sections = processRawSections(rawSections);
    
    return {
      sections: sections,
      selectedSection: sections[0] || null,
      selectedDoc: sections[0] && sections[0].items[0] || null,
      
      getSectionBySlug: function(sectionSlug) {
        return sections.find(function(s) { return s.slug === sectionSlug; }) || null;
      },
      
      getDocBySlug: function(sectionSlug, docSlug) {
        var section = sections.find(function(s) { return s.slug === sectionSlug; });
        if (!section) return null;
        return section.items.find(function(d) { return d.slug === docSlug; }) || null;
      },
      
      getNextDoc: function(currentDoc) {
        if (!currentDoc) return null;
        var currentSection = sections.find(function(s) { return s.slug === currentDoc.sectionSlug; });
        if (!currentSection) return null;
        var idx = currentSection.items.findIndex(function(d) { return d.slug === currentDoc.slug; });
        if (idx < currentSection.items.length - 1) return currentSection.items[idx + 1];
        var secIdx = sections.findIndex(function(s) { return s.slug === currentSection.slug; });
        if (secIdx < sections.length - 1) return sections[secIdx + 1].items[0] || null;
        return null;
      },
      
      getPrevDoc: function(currentDoc) {
        if (!currentDoc) return null;
        var currentSection = sections.find(function(s) { return s.slug === currentDoc.sectionSlug; });
        if (!currentSection) return null;
        var idx = currentSection.items.findIndex(function(d) { return d.slug === currentDoc.slug; });
        if (idx > 0) return currentSection.items[idx - 1];
        var secIdx = sections.findIndex(function(s) { return s.slug === currentSection.slug; });
        if (secIdx > 0) {
          var prev = sections[secIdx - 1];
          return prev.items[prev.items.length - 1] || null;
        }
        return null;
      },
      
      buildDocUrl: function(sectionSlug, docSlug) {
        if (!docSlug || docSlug === 'index') return '/documentation/' + sectionSlug;
        return '/documentation/' + sectionSlug + '/' + docSlug;
      }
    };
  }

  // ============================================
  // Virtual DOM Helper for JSX-style expressions
  // ============================================
  
  const SVG_NAMESPACE = 'http://www.w3.org/2000/svg';
  const SVG_TAGS = new Set([
      'svg', 'path', 'circle', 'ellipse', 'line', 'polyline', 'polygon', 'rect',
      'g', 'defs', 'clipPath', 'mask', 'use', 'symbol', 'text', 'tspan', 'textPath',
      'image', 'foreignObject', 'switch', 'desc', 'title', 'metadata', 'linearGradient',
      'radialGradient', 'stop', 'pattern', 'filter', 'feBlend', 'feColorMatrix',
      'feComponentTransfer', 'feComposite', 'feConvolveMatrix', 'feDiffuseLighting',
      'feDisplacementMap', 'feFlood', 'feGaussianBlur', 'feImage', 'feMerge',
      'feMergeNode', 'feMorphology', 'feOffset', 'feSpecularLighting', 'feTile',
      'feTurbulence', 'animate', 'animateMotion', 'animateTransform', 'set', 'marker'
  ]);

  // Track current namespace context for nested elements
  let currentNamespace = null;

  function h(tag, props, children) {
    // Determine if this element should be in SVG namespace
    const isSvgTag = SVG_TAGS.has(tag) || SVG_TAGS.has(tag.toLowerCase());
    const useSvgNamespace = isSvgTag || currentNamespace === SVG_NAMESPACE;
    
    // Create element with correct namespace
    const el = useSvgNamespace 
        ? document.createElementNS(SVG_NAMESPACE, tag)
        : document.createElement(tag);
    
    // Track namespace context for children (save previous, set new if svg root)
    const previousNamespace = currentNamespace;
    if (tag === 'svg' || tag === 'SVG') {
        currentNamespace = SVG_NAMESPACE;
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

      for (const [key, value] of Object.entries(props)) {
        if (key.startsWith('on') && typeof value === 'function') {
          el.addEventListener(key.slice(2).toLowerCase(), value);
        } else if (key === 'class' || key === 'className') {
          if (typeof value === 'function') {
            // Reactive class binding
            zenEffect(function() {
              setClass(el, value());
            });
          } else {
            setClass(el, value);
          }
        } else if (key === 'style' && typeof value === 'object') {
          Object.assign(el.style, value);
        } else if (typeof value === 'function') {
          // Reactive attribute binding
          zenEffect(function() {
            const v = value();
            if (v != null && v !== false) {
              el.setAttribute(key, String(v));
            } else {
              el.removeAttribute(key);
            }
          });
        } else if (value != null && value !== false) {
          el.setAttribute(key, String(value));
        }
      }
    }
    
    if (children != null && children !== false) {
      // Flatten nested arrays (from .map() calls)
      const childrenArray = Array.isArray(children) ? children.flat(Infinity) : [children];
      for (const child of childrenArray) {
        // Skip null, undefined, and false
        if (child == null || child === false) continue;
        
        if (typeof child === 'function') {
          // Reactive child - use a placeholder and update reactively
          const placeholder = document.createComment('expr');
          el.appendChild(placeholder);
          let currentNodes = [];
          
          zenEffect(function() {
            const result = child();
            // Remove old nodes
            for (let i = 0; i < currentNodes.length; i++) {
              const n = currentNodes[i];
              if (n.parentNode) n.parentNode.removeChild(n);
            }
            currentNodes = [];
            
            if (result == null || result === false) {
              // Render nothing
            } else if (result instanceof Node) {
              placeholder.parentNode.insertBefore(result, placeholder);
              currentNodes = [result];
            } else if (Array.isArray(result)) {
              // Array of nodes/strings
              const flat = result.flat(Infinity);
              for (let i = 0; i < flat.length; i++) {
                const item = flat[i];
                if (item == null || item === false) continue;
                const node = item instanceof Node ? item : document.createTextNode(String(item));
                placeholder.parentNode.insertBefore(node, placeholder);
                currentNodes.push(node);
              }
            } else {
              // Primitive (string, number)
              const textNode = document.createTextNode(String(result));
              placeholder.parentNode.insertBefore(textNode, placeholder);
              currentNodes = [textNode];
            }
          });
        } else {
          // Static child
          el.appendChild(child instanceof Node ? child : document.createTextNode(String(child)));
        }
      }
    }
    
    // Restore previous namespace context
    currentNamespace = previousNamespace;

    return el;
  }


  // ============================================
  // Export to window.__zenith
  // ============================================
  
  global.__zenith = {
    // Reactivity primitives
    signal: zenSignal,
    state: zenState,
    effect: zenEffect,
    memo: zenMemo,
    ref: zenRef,
    batch: zenBatch,
    untrack: zenUntrack,
    // zenith:content
    defineSchema: defineSchema,
    zenCollection: zenCollection,
    // useZenOrder hook
    createZenOrder: createZenOrder,
    processRawSections: processRawSections,
    slugify: slugify,
    // Virtual DOM helper for JSX
    h: h,
    // Lifecycle
    onMount: zenOnMount,
    onUnmount: zenOnUnmount,
    // Internal hooks
    triggerMount: triggerMount,
    triggerUnmount: triggerUnmount,
    // Hydration
    hydrate: zenithHydrate,
    hydrateComponents: hydrateComponents,  // Marker-driven component instantiation
    registerExpression: registerExpression,
    getExpression: getExpression,
    // Component instance system
    createInstance: createComponentInstance,
    defineComponent: defineComponent,
    instantiate: instantiateComponent
  };
  
  // Expose with zen* prefix for direct usage
  global.zenSignal = zenSignal;
  global.zenState = zenState;
  global.zenEffect = zenEffect;
  global.zenMemo = zenMemo;
  global.zenRef = zenRef;
  global.zenBatch = zenBatch;
  global.zenUntrack = zenUntrack;
  global.zenOnMount = zenOnMount;
  global.zenOnUnmount = zenOnUnmount;
  global.zenithHydrate = zenithHydrate;
  
  // Clean aliases
  global.signal = zenSignal;
  global.state = zenState;
  global.effect = zenEffect;
  global.memo = zenMemo;
  global.ref = zenRef;
  global.batch = zenBatch;
  global.untrack = zenUntrack;
  global.onMount = zenOnMount;
  global.onUnmount = zenOnUnmount;
  
  // useZenOrder hook exports
  global.createZenOrder = createZenOrder;
  global.processRawSections = processRawSections;
  global.slugify = slugify;
  
  // ============================================
  // SPA Router Runtime
  // ============================================
  
  // Router state
    // Current route state
    var currentRoute = {
      path: '/',
      params: {},
      query: {}
    };
    
    // Route listeners
    var routeListeners = new Set();
    
    // Router outlet element
    var routerOutlet = null;
    
    // Page modules registry
    var pageModules = {};
    
    // Route manifest
    var routeManifest = [];
    
    /**
     * Parse query string
     */
    function parseQueryString(search) {
      var query = {};
      if (!search || search === '?') return query;
      var params = new URLSearchParams(search);
      params.forEach(function(value, key) { query[key] = value; });
      return query;
    }
    
  // ============================================
  // Global Error Listeners (Dev Mode)
  // ============================================

  window.onerror = function(message, source, lineno, colno, error) {
    renderErrorPage(error || new Error(message), {
      file: source,
      line: lineno,
      column: colno,
      errorType: 'UncaughtError'
    });
    return false;
  };

  window.onunhandledrejection = function(event) {
    renderErrorPage(event.reason || new Error('Unhandled Promise Rejection'), {
      errorType: 'UnhandledRejection'
    });
  };

  /**
   * Resolve route from pathname
   */
    function resolveRoute(pathname) {
      var normalizedPath = pathname === '' ? '/' : pathname;
      
      for (var i = 0; i < routeManifest.length; i++) {
        var route = routeManifest[i];
        var match = route.regex.exec(normalizedPath);
        if (match) {
          var params = {};
          for (var j = 0; j < route.paramNames.length; j++) {
            var paramValue = match[j + 1];
            if (paramValue !== undefined) {
              params[route.paramNames[j]] = decodeURIComponent(paramValue);
            }
          }
          return { record: route, params: params };
        }
      }
      return null;
    }
    
    /**
     * Clean up previous page
     */
    function cleanupPreviousPage() {
      // Trigger unmount lifecycle hooks
      if (global.__zenith && global.__zenith.triggerUnmount) {
        global.__zenith.triggerUnmount();
      }
      
      // Remove previous page styles
      var prevStyles = document.querySelectorAll('style[data-zen-page-style]');
      prevStyles.forEach(function(s) { s.remove(); });
      
      // Clean up window properties
      if (global.__zenith_cleanup) {
        global.__zenith_cleanup.forEach(function(key) {
          try { delete global[key]; } catch(e) {}
        });
      }
      global.__zenith_cleanup = [];
    }
    
    /**
     * Inject styles
     */
    function injectStyles(styles) {
      styles.forEach(function(content, i) {
        var style = document.createElement('style');
        style.setAttribute('data-zen-page-style', String(i));
        style.textContent = content;
        document.head.appendChild(style);
      });
    }
    
    /**
     * Execute scripts
     */
    function executeScripts(scripts) {
      scripts.forEach(function(content) {
        try {
          var fn = new Function(content);
          fn();
        } catch (e) {
          console.error('[Zenith Router] Script error:', e);
        }
      });
    }
    
    /**
     * Render page
     */
    function renderPage(pageModule) {
      if (!routerOutlet) {
        console.warn('[Zenith Router] No router outlet');
        return;
      }
      
      cleanupPreviousPage();
      routerOutlet.innerHTML = pageModule.html;
      injectStyles(pageModule.styles);
      executeScripts(pageModule.scripts);
      
      // Trigger mount lifecycle hooks after scripts are executed
      if (global.__zenith && global.__zenith.triggerMount) {
        global.__zenith.triggerMount();
      }
    }
    
    /**
     * Notify listeners
     */
    function notifyListeners(route, prevRoute) {
      routeListeners.forEach(function(listener) {
        try { listener(route, prevRoute); } catch(e) {}
      });
    }
    
    /**
     * Resolve and render
     */
    function resolveAndRender(path, query, updateHistory, replace) {
      replace = replace || false;
      var prevRoute = Object.assign({}, currentRoute);
      var resolved = resolveRoute(path);
      
      if (resolved) {
        currentRoute = {
          path: path,
          params: resolved.params,
          query: query,
          matched: resolved.record
        };
        
        var pageModule = pageModules[resolved.record.path];
        if (pageModule) {
          renderPage(pageModule);
        }
      } else {
        currentRoute = { path: path, params: {}, query: query, matched: undefined };
        console.warn('[Zenith Router] No route matched:', path);
        
        // Render 404 if available
        if (routerOutlet) {
          routerOutlet.innerHTML = '<div style="padding: 2rem; text-align: center;"><h1>404</h1><p>Page not found</p></div>';
        }
      }
      
      if (updateHistory) {
        var url = path + (Object.keys(query).length ? '?' + new URLSearchParams(query) : '');
        if (replace) {
          history.replaceState(null, '', url);
        } else {
          history.pushState(null, '', url);
        }
      }
      
      notifyListeners(currentRoute, prevRoute);
      global.__zenith_route = currentRoute;
    }
    
    /**
     * Handle popstate
     */
    function handlePopState() {
      resolveAndRender(
        location.pathname,
        parseQueryString(location.search),
        false,
        false
      );
    }
    
    /**
     * Navigate (public API)
     */
    function navigate(to, options) {
      options = options || {};
      var path, query = {};
      
      if (to.includes('?')) {
        var parts = to.split('?');
        path = parts[0];
        query = parseQueryString('?' + parts[1]);
      } else {
        path = to;
      }
      
      if (!path.startsWith('/')) {
        var currentDir = currentRoute.path.split('/').slice(0, -1).join('/');
        path = currentDir + '/' + path;
      }
      
      var normalizedPath = path === '' ? '/' : path;
      var currentPath = currentRoute.path === '' ? '/' : currentRoute.path;
      var isSamePath = normalizedPath === currentPath;
      
      if (isSamePath && JSON.stringify(query) === JSON.stringify(currentRoute.query)) {
        return;
      }
      
      // Dev mode: If no route manifest is loaded, use browser navigation
      // This allows ZenLink to work in dev server where pages are served fresh
      if (routeManifest.length === 0) {
        var url = normalizedPath + (Object.keys(query).length ? '?' + new URLSearchParams(query) : '');
        if (options.replace) {
          location.replace(url);
        } else {
          location.href = url;
        }
        return;
      }
      
      resolveAndRender(path, query, true, options.replace || false);
    }
    
    /**
     * Get current route
     */
    function getRoute() {
      return Object.assign({}, currentRoute);
    }
    
    /**
     * Subscribe to route changes
     */
    function onRouteChange(listener) {
      routeListeners.add(listener);
      return function() { routeListeners.delete(listener); };
    }
    
    /**
     * Check if path is active
     */
    function isActive(path, exact) {
      if (exact) return currentRoute.path === path;
      return currentRoute.path.startsWith(path);
    }
    
    /**
     * Prefetch a route
     */
    var prefetchedRoutes = new Set();
    function prefetch(path) {
      var normalizedPath = path === '' ? '/' : path;
      
      if (prefetchedRoutes.has(normalizedPath)) {
        return Promise.resolve();
      }
      prefetchedRoutes.add(normalizedPath);
      
      var resolved = resolveRoute(normalizedPath);
      if (!resolved) {
        return Promise.resolve();
      }
      
      // In SPA build, all modules are already loaded
      return Promise.resolve();
    }
    
    /**
     * Initialize router
     */
    function initRouter(manifest, modules, outlet) {
      routeManifest = manifest;
      Object.assign(pageModules, modules);
      
      if (outlet) {
        routerOutlet = typeof outlet === 'string' 
          ? document.querySelector(outlet) 
          : outlet;
      }
      
      window.addEventListener('popstate', handlePopState);
      
      // Initial route resolution
      resolveAndRender(
        location.pathname,
        parseQueryString(location.search),
        false
      );
    }
    
  // Expose router API globally
  global.__zenith_router = {
    navigate: navigate,
    getRoute: getRoute,
    onRouteChange: onRouteChange,
    isActive: isActive,
    prefetch: prefetch,
    initRouter: initRouter
  };
  
  // Also expose navigate directly for convenience
  global.navigate = navigate;
  global.zenCollection = zenCollection;
  
  
  // ============================================
  // HMR Client (Development Only)
  // ============================================
  
  if (typeof window !== 'undefined' && (location.hostname === 'localhost' || location.hostname === '127.0.0.1')) {
    let socket;
    function connectHMR() {
      const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      socket = new WebSocket(protocol + '//' + location.host + '/hmr');
      
      socket.onmessage = function(event) {
        try {
          const data = JSON.parse(event.data);
          if (data.type === 'reload') {
            console.log('[Zenith] HMR: Reloading page...');
            location.reload();
          } else if (data.type === 'style-update') {
            console.log('[Zenith] HMR: Updating style ' + data.url);
            const links = document.querySelectorAll('link[rel="stylesheet"]');
            for (let i = 0; i < links.length; i++) {
              const link = links[i];
              const url = new URL(link.href);
              if (url.pathname === data.url) {
                link.href = data.url + '?t=' + Date.now();
                break;
              }
            }
          }
        } catch (e) {
          console.error('[Zenith] HMR Error:', e);
        }
      };
      
      socket.onclose = function() {
        console.log('[Zenith] HMR: Connection closed. Retrying in 2s...');
        setTimeout(connectHMR, 2000);
      };
    }
    
    // Connect unless explicitly disabled
    if (!window.__ZENITH_NO_HMR__) {
      connectHMR();
    }
  }
  
})(typeof window !== 'undefined' ? window : this);
`
}

/**
 * Generate a minified version of the bundle
 * For production builds
 */
export function generateMinifiedBundleJS(): string {
  // For now, return non-minified
  // TODO: Add minification via terser or similar
  return generateBundleJS()
}

/**
 * Get bundle version for cache busting
 */
export function getBundleVersion(): string {
  return '0.1.0'
}
