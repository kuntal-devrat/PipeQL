import React, { useState, useEffect, useRef, useCallback } from 'react';
import { SAMPLE_QUERIES } from './data/docsContent';
import { Database, Shield, Copy, Check, Play, Terminal, Cpu, FileCode, Layers, Info, RefreshCw, Code2, Zap } from 'lucide-react';
import { initWasm, compile as pipeqlCompile } from './wasm.js';

// ─── Search Index ───────────────────────────────────────────────────
const DOC_SECTIONS_DB = [
  { id: 'intro', tab: 'docs', title: 'Introduction to PipeQL', breadcrumbs: 'Docs > Getting Started > Introduction', snippet: 'PipeQL is a compiled query language for safe, high-speed relational transactions with 100% parameter isolation.', keywords: ['intro', 'philosophy', 'about', 'concept', 'performance', 'overview'] },
  { id: 'quickstart', tab: 'docs', title: 'Quick Start & Installation', breadcrumbs: 'Docs > Getting Started > Quick Start', snippet: 'Install PipeQL via npm, pip, or cargo. Set up database drivers in under 5 minutes.', keywords: ['install', 'setup', 'npm', 'pip', 'quick', 'start', 'driver', 'connection'] },
  { id: 'tutorial-1', tab: 'docs', title: 'Tutorial: Your First Query', breadcrumbs: 'Docs > Tutorial > Step 1', snippet: 'Step-by-step guide to writing your first PipeQL query and seeing the compiled SQL output.', keywords: ['tutorial', 'first', 'beginner', 'hello', 'start', 'learn', 'step'] },
  { id: 'tutorial-2', tab: 'docs', title: 'Tutorial: Filtering & Parameters', breadcrumbs: 'Docs > Tutorial > Step 2', snippet: 'Learn how to filter data and use $parameters for safe, injection-proof queries.', keywords: ['tutorial', 'filter', 'where', 'parameter', '$', 'condition'] },
  { id: 'tutorial-3', tab: 'docs', title: 'Tutorial: Joins & Groups', breadcrumbs: 'Docs > Tutorial > Step 3', snippet: 'Combine tables with joins and aggregate data with group operations.', keywords: ['tutorial', 'join', 'group', 'aggregate', 'combine', 'sum', 'count'] },
  { id: 'tutorial-4', tab: 'docs', title: 'Tutorial: Writing Data', breadcrumbs: 'Docs > Tutorial > Step 4', snippet: 'Insert, update, and delete records using PipeQL mutation syntax.', keywords: ['tutorial', 'insert', 'update', 'delete', 'write', 'mutation'] },
  { id: 'tutorial-5', tab: 'docs', title: 'Tutorial: Real App', breadcrumbs: 'Docs > Tutorial > Step 5', snippet: 'Build a complete CRUD API with PipeQL driver adapters.', keywords: ['tutorial', 'app', 'api', 'crud', 'driver', 'real', 'example'] },
  { id: 'syntax', tab: 'docs', title: 'Query Syntax Reference', breadcrumbs: 'Docs > Syntax Reference > Query Syntax', snippet: 'Formal EBNF grammar for all PipeQL statement types.', keywords: ['syntax', 'grammar', 'ebnf', 'rules', 'pipeline'] },
  { id: 'mutations', tab: 'docs', title: 'Mutations (DML)', breadcrumbs: 'Docs > Syntax Reference > Mutations', snippet: 'Insert, update, and delete statements with full parameter isolation.', keywords: ['insert', 'update', 'delete', 'mutation', 'dml', 'all', 'delete all', 'update all', 'escape hatch', 'full table'] },
  { id: 'upsert', tab: 'docs', title: 'Upsert (v1.1)', breadcrumbs: 'Docs > Syntax Reference > Upsert', snippet: 'Insert or update on conflict. Uses ON CONFLICT DO UPDATE or ON DUPLICATE KEY UPDATE.', keywords: ['upsert', 'conflict', 'insert or update', 'on conflict', 'duplicate key', 'v1.1'] },
  { id: 'union', tab: 'docs', title: 'Union (v1.1)', breadcrumbs: 'Docs > Syntax Reference > Union', snippet: 'Combine results from multiple queries with UNION or UNION ALL.', keywords: ['union', 'combine', 'all', 'distinct', 'v1.1'] },
  { id: 'subquery', tab: 'docs', title: 'Subqueries (v1.1)', breadcrumbs: 'Docs > Syntax Reference > Subqueries', snippet: 'Nested queries using IN subquery syntax for filtering.', keywords: ['subquery', 'in', 'nested', 'filter', 'v1.1'] },
  { id: 'ddl', tab: 'docs', title: 'Table Schema (DDL)', breadcrumbs: 'Docs > Syntax Reference > DDL', snippet: 'Create tables with typed columns, primary keys, defaults, and auto-increment.', keywords: ['table', 'ddl', 'create', 'schema', 'column'] },
  { id: 'api-reference', tab: 'docs', title: 'API Reference', breadcrumbs: 'Docs > Polyglot SDKs > API Reference', snippet: 'Compile, parse, and version APIs for JavaScript, Python, C, Go, and CLI.', keywords: ['api', 'compile', 'parse', 'version', 'function', 'sdk', 'cli'] },
  { id: 'drivers', tab: 'docs', title: 'Driver Adapters', breadcrumbs: 'Docs > Polyglot SDKs > Drivers', snippet: 'Zero-boilerplate database wrappers for Node.js and Python.', keywords: ['driver', 'adapter', 'sqlite', 'postgres', 'mysql', 'duckdb'] },
  { id: 'builder', tab: 'docs', title: 'Fluent Builder (Optional)', breadcrumbs: 'Docs > Polyglot SDKs > Fluent Builder', snippet: 'The PipeQL string DSL is the primary interface. For programmatic composition, every SDK ships an optional fluent builder that composes the same source string.', keywords: ['builder', 'fluent', 'chain', 'compose', 'query builder', 'object insert', 'sdk', 'optional', 'programmatic'] },
  { id: 'lsp', tab: 'docs', title: 'LSP & VS Code', breadcrumbs: 'Docs > Tools & IDE > LSP & VS Code', snippet: 'Language server protocol with diagnostics, completion, and VS Code extension.', keywords: ['lsp', 'vscode', 'extension', 'ide', 'diagnostics'] },
  { id: 'tree-sitter', tab: 'docs', title: 'Tree-sitter Grammar', breadcrumbs: 'Docs > Tools & IDE > Tree-sitter', snippet: 'Syntax highlighting and parsing grammar for PipeQL.', keywords: ['tree-sitter', 'grammar', 'highlight', 'parser'] },
  { id: 'architecture', tab: 'docs', title: 'Architecture & Security', breadcrumbs: 'Docs > Deep Dive > Architecture', snippet: 'Three-stage compilation pipeline with AST-level parameter isolation.', keywords: ['architecture', 'ast', 'compiler', 'isolation', 'security', 'injection', 'rust'] },
  { id: 'contributing', tab: 'docs', title: 'Contributing', breadcrumbs: 'Docs > Deep Dive > Contributing', snippet: 'How to build, test, and contribute to PipeQL.', keywords: ['contributing', 'build', 'test', 'develop', 'cargo'] }
];

const SUGGESTIONS = [
  'how to install pipeql', 'pipeql vs sql injection', 'first query tutorial',
  'filter with parameters', 'join tables', 'insert data', 'create table',
  'javascript driver', 'python driver', 'vs code extension', 'architecture'
];

// ─── Reusable Components ────────────────────────────────────────────
function CodeBlock({ children, label, className = '' }) {
  const [copied, setCopied] = useState(false);
  const [scrollable, setScrollable] = useState(false);
  const ref = React.useRef(null);
  const copy = () => { navigator.clipboard.writeText(children); setCopied(true); setTimeout(() => setCopied(false), 2000); };
  React.useEffect(() => {
    const el = ref.current;
    if (el) setScrollable(el.scrollWidth > el.clientWidth);
  }, [children]);
  return (
    <div className={`rounded-2xl border border-outline-variant/20 min-w-0 ${className}`}>
      {label && (
        <div className="bg-surface-container-high px-4 py-2 border-b border-outline-variant/20 text-[10px] font-bold uppercase tracking-wider text-on-surface-variant flex items-center justify-between rounded-t-2xl">
          <span>{label}</span>
          <button onClick={copy} className="flex items-center gap-1 text-outline hover:text-primary transition-colors">
            {copied ? <Check size={12} className="text-g-green" /> : <Copy size={12} />}
            {copied ? 'Copied' : 'Copy'}
          </button>
        </div>
      )}
      <div ref={ref} className={`bg-surface-container-lowest font-mono text-[13px] text-on-surface overflow-x-auto rounded-b-2xl code-scroll-wrapper ${scrollable ? 'scrollable' : ''}`}>
        <pre className="p-4 leading-relaxed whitespace-pre">{children}</pre>
      </div>
    </div>
  );
}

function InlineCode({ children }) {
  return <code className="bg-surface-container px-1.5 py-0.5 rounded-lg text-xs font-mono text-primary">{children}</code>;
}

function SectionTitle({ children }) {
  return <h2 className="text-lg font-bold text-on-surface">{children}</h2>;
}

function Warning({ children, type = 'error' }) {
  const s = type === 'error' ? 'bg-[#ba1a1a]/8 border-[#ba1a1a]/20 text-[#ba1a1a]' : 'bg-primary-fixed/30 border-primary/20 text-on-surface';
  return <div className={`${s} p-4 rounded-2xl text-sm flex gap-3 items-start border`}><span className="material-symbols-outlined text-base mt-0.5">{type === 'error' ? 'warning' : 'info'}</span><div>{children}</div></div>;
}

function StepCard({ num, title, children }) {
  return (
    <div className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl p-5 relative">
      <div className="absolute -top-3 -left-1 w-7 h-7 rounded-xl bg-primary text-on-primary flex items-center justify-center text-xs font-bold shadow-sm">{num}</div>
      <h4 className="text-sm font-bold text-on-surface mb-2 mt-1">{title}</h4>
      <div className="text-sm text-on-surface-variant leading-relaxed">{children}</div>
    </div>
  );
}

// ─── Sidebar Config ─────────────────────────────────────────────────
const SIDEBAR = [
  { group: 'Getting Started', items: [['intro','Introduction'],['quickstart','Quick Start']] },
  { group: 'Tutorial', items: [['tutorial-1','Step 1: First Query'],['tutorial-2','Step 2: Filters & Params'],['tutorial-3','Step 3: Joins & Groups'],['tutorial-4','Step 4: Writing Data'],['tutorial-5','Step 5: Real App']] },
  { group: 'Syntax Reference', items: [['syntax','Query Syntax'],['mutations','Mutations (DML)'],['upsert','Upsert (v1.1)'],['union','Union (v1.1)'],['subquery','Subqueries (v1.1)'],['ddl','Table Schema (DDL)']] },
  { group: 'Polyglot SDKs', items: [['api-reference','API Reference'],['drivers','Driver Adapters'],['builder','Fluent Builder (Optional)']] },
  { group: 'Tools & IDE', items: [['lsp','LSP & VS Code'],['tree-sitter','Tree-sitter']] },
  { group: 'Deep Dive', items: [['architecture','Architecture'],['contributing','Contributing']] }
];

// ─── App ────────────────────────────────────────────────────────────
export default function App() {
  const [activeTab, setActiveTab] = useState('home');
  const [activeDocSection, setActiveDocSection] = useState('intro');
  const [compilerDialect, setCompilerDialect] = useState('postgres');
  const [copiedCode, setCopiedCode] = useState(false);
  const [searchValue, setSearchValue] = useState('');
  const [searchResults, setSearchResults] = useState(null);
  const [searchInput, setSearchInput] = useState('');

  // Playground state
  const [editorValue, setEditorValue] = useState(SAMPLE_QUERIES.basic.pipeql);
  const [compileResult, setCompileResult] = useState(null);
  const [compileError, setCompileError] = useState(null);
  const [wasmReady, setWasmReady] = useState(false);
  const [compileTime, setCompileTime] = useState(null);
  const [outputTab, setOutputTab] = useState('sql'); // 'sql' | 'params' | 'ast'
  const [isCompiling, setIsCompiling] = useState(false);
  const textareaRef = useRef(null);

  const handleRun = useCallback(async (codeToCompile = editorValue, dialectToUse = compilerDialect) => {
    if (!codeToCompile || !codeToCompile.trim()) return;
    setIsCompiling(true);
    const start = performance.now();
    try {
      const result = await pipeqlCompile(codeToCompile, dialectToUse);
      setCompileResult(result);
      setCompileError(null);
      setCompileTime((performance.now() - start).toFixed(2));
    } catch (err) {
      setCompileError(err?.message || String(err));
      setCompileResult(null);
      setCompileTime(null);
    } finally {
      setIsCompiling(false);
    }
  }, [editorValue, compilerDialect]);

  // Initialize WASM on mount
  useEffect(() => {
    initWasm()
      .then(() => {
        setWasmReady(true);
      })
      .catch((err) => {
        console.error('Failed to load WASM engine:', err);
      });
  }, []);

  // Auto compile on initial load & when WASM ready or dialect changes
  useEffect(() => {
    if (wasmReady) {
      handleRun(editorValue, compilerDialect);
    }
  }, [wasmReady, compilerDialect]);

  const loadSample = (key) => {
    const val = SAMPLE_QUERIES[key].pipeql;
    setEditorValue(val);
    if (wasmReady) {
      handleRun(val, compilerDialect);
    }
  };

  const handleKeyDown = (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      handleRun();
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      const ta = e.target;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      setEditorValue(editorValue.substring(0, start) + '  ' + editorValue.substring(end));
      setTimeout(() => { ta.selectionStart = ta.selectionEnd = start + 2; }, 0);
    }
  };

  const copyToClipboard = (text) => {
    navigator.clipboard.writeText(text).then(() => {
      setCopiedCode(true);
      setTimeout(() => setCopiedCode(false), 2000);
    });
  };

  const doSearch = (q) => {
    const query = (q || searchValue).trim().toLowerCase();
    if (!query) return;
    const matches = DOC_SECTIONS_DB.filter(s =>
      s.title.toLowerCase().includes(query) || s.snippet.toLowerCase().includes(query) || s.keywords.some(k => k.includes(query))
    );
    setSearchResults({ query, matches });
    setSearchValue(query);
    setActiveTab('search-results');
  };

  const handleSearchSubmit = (e) => { e.preventDefault(); doSearch(); };

  const goToDoc = (id) => { setActiveTab('docs'); setActiveDocSection(id); setSearchResults(null); setSearchValue(''); window.scrollTo(0,0); };

  const handleSearchResultClick = (tab, id) => { goToDoc(id); };

  // ─── Sidebar Component ──────────────────────────────────────────
  const Sidebar = () => (
    <aside className="hidden lg:flex flex-col w-56 shrink-0 self-start sticky top-[52px] h-[calc(100vh-52px)] py-5 bg-background border-r border-surface-container overflow-y-auto" style={{scrollbarWidth:'none'}}>
      <div className="mb-3 px-4">
        <h2 className="text-base font-bold text-on-surface">Docs</h2>
        <span className="text-[9px] text-on-surface-variant font-medium mt-0.5 block">v1.1.7</span>
      </div>
      <nav className="flex-1 flex flex-col gap-1">
        {SIDEBAR.map(({ group, items }) => (
          <div key={group} className="px-2">
            <div className="text-[9px] font-bold uppercase tracking-widest text-outline pl-5 py-1.5">{group}</div>
            <div className="relative ml-2 border-l border-outline-variant/25">
              {items.map(([id, label], idx) => {
                const isActive = activeDocSection === id && activeTab === 'docs';
                const isLast = idx === items.length - 1;
                return (
                  <div key={id} className="relative">
                    {/* Curved branch from trunk to item */}
                    <div className="absolute left-0 top-[11px] w-3 h-2 border-l-2 border-b-2 border-outline-variant/25 rounded-bl-lg" style={{borderColor: isActive ? 'var(--color-primary)' : undefined}}></div>
                    <button onClick={() => goToDoc(id)}
                      className={`relative z-10 flex items-center w-full pl-5 pr-2 py-[5px] text-left text-xs transition-all rounded-r-lg ${isActive ? 'text-primary font-bold bg-primary-fixed/20' : 'text-on-surface-variant hover:bg-surface-container'}`}>
                      {label}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </nav>
    </aside>
  );

  return (
    <div className="bg-background text-on-background font-sans antialiased flex flex-col min-h-screen overflow-hidden">
      {/* ─── Navbar ─── */}
      <header className="sticky top-0 w-full z-50 flex justify-between items-center px-6 py-3 bg-surface/90 backdrop-blur-sm shadow-sm border-b border-surface-container">
        <div className="flex items-center gap-5">
          <a className="flex items-center gap-2 cursor-pointer select-none" onClick={() => { setActiveTab('home'); setSearchResults(null); }}>
            <img src="/logo.png" alt="PipeQL" className="h-7 w-auto rounded-lg" />
            <span className="tracking-tight text-lg font-bold">
              <span className="g-blue">P</span><span className="g-red">i</span><span className="g-yellow">p</span><span className="g-blue">e</span><span className="g-green">Q</span><span className="g-red">L</span>
            </span>
          </a>
          <nav className="hidden md:flex gap-2 ml-6">
            {[['home','Overview'],['about','About'],['features','Features'],['docs','Docs']].map(([t,l]) => (
              <button key={t} onClick={() => t === 'docs' ? goToDoc('intro') : setActiveTab(t)}
                className={`transition-colors font-semibold px-3.5 py-1.5 rounded-2xl text-xs duration-200 ${activeTab === t ? 'text-primary bg-primary-fixed/30' : 'text-on-surface-variant hover:text-primary hover:bg-surface-container-low'}`}>{l}</button>
            ))}
          </nav>
        </div>
        <a href="https://github.com/Flaxmbot/PipeQL" target="_blank" rel="noreferrer"
          className="border border-outline-variant hover:bg-surface-container bg-surface text-on-surface px-4 py-1.5 rounded-2xl text-xs font-semibold transition-all flex items-center gap-1.5">
          <span className="material-symbols-outlined text-sm">terminal</span> GitHub
        </a>
      </header>

      {/* ─── Main ─── */}
      <main className="flex-grow flex">

        {/* ═══════════════════════════════════════════════════════════ */}
        {/* SEARCH RESULTS (Google-style)                             */}
        {/* ═══════════════════════════════════════════════════════════ */}
        {activeTab === 'search-results' && searchResults && (
          <div className="flex flex-col w-full overflow-y-auto" style={{scrollbarWidth:'none'}}>
            <div className="max-w-2xl mx-auto px-6 py-10 w-full">
              {/* Logo + Search bar */}
              <div className="flex items-center gap-4 mb-8">
                <img src="/logo.png" alt="" className="h-8 w-auto rounded-lg" />
                <form onSubmit={handleSearchSubmit} className="flex-1 relative">
                  <input className="w-full px-4 py-2.5 rounded-full border border-outline-variant bg-surface-container-lowest text-sm focus:ring-2 focus:ring-primary outline-none"
                    value={searchValue} onChange={e => setSearchValue(e.target.value)} type="text" />
                  <button type="submit" className="absolute right-3 top-1/2 -translate-y-1/2 text-outline hover:text-primary">
                    <span className="material-symbols-outlined text-xl">search</span>
                  </button>
                </form>
              </div>

              {/* Tabs */}
              <div className="flex gap-6 border-b border-outline-variant/30 mb-6 text-sm">
                <button className="pb-2 border-b-2 border-primary text-primary font-semibold">All</button>
                <button className="pb-2 text-on-surface-variant hover:text-on-surface">Docs</button>
                <button className="pb-2 text-on-surface-variant hover:text-on-surface">Tutorials</button>
              </div>

              <p className="text-xs text-outline mb-5">About {searchResults.matches.length} results (0.04 seconds)</p>

              {searchResults.matches.length > 0 ? (
                <div className="space-y-6">
                  {searchResults.matches.map(m => (
                    <div key={m.id} className="group">
                      <div className="text-[11px] text-on-surface-variant mb-0.5">{m.breadcrumbs}</div>
                      <a onClick={() => handleSearchResultClick(m.tab, m.id)}
                        className="text-lg text-[#1a0dab] hover:underline cursor-pointer font-medium block leading-snug">{m.title}</a>
                      <p className="text-sm text-on-surface-variant leading-relaxed mt-0.5">{m.snippet}</p>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="text-center py-16">
                  <span className="material-symbols-outlined text-5xl text-outline mb-3 block">search_off</span>
                  <h3 className="text-base font-semibold text-on-surface mb-1">No results for "{searchResults.query}"</h3>
                  <p className="text-sm text-on-surface-variant">Try different keywords or check spelling.</p>
                </div>
              )}

              {/* Suggestions */}
              <div className="mt-10 pt-6 border-t border-outline-variant/30">
                <p className="text-xs font-semibold text-outline uppercase tracking-wider mb-3">Related searches</p>
                <div className="flex flex-wrap gap-2">
                  {SUGGESTIONS.filter(s => !s.includes(searchResults.query)).slice(0,6).map(s => (
                    <button key={s} onClick={() => doSearch(s)}
                      className="text-sm text-primary hover:underline cursor-pointer">{s}</button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* ═══════════════════════════════════════════════════════════ */}
        {/* HOME                                                      */}
        {/* ═══════════════════════════════════════════════════════════ */}
        {activeTab === 'home' && (
          <div className="flex flex-col items-center w-full overflow-y-auto" style={{scrollbarWidth:'none'}}>
            <section className="relative w-full min-h-[65vh] flex flex-col items-center justify-center pt-10 pb-14 px-6 bg-grid-pattern border-b border-surface-container">
              <div className="relative z-10 max-w-3xl w-full flex flex-col items-center text-center space-y-5">
                <div className="inline-flex items-center gap-2 px-3 py-1 rounded-2xl bg-surface-container border border-outline-variant/30 text-on-surface-variant text-[11px] font-semibold fade-in-up cursor-pointer hover:bg-surface-container-high transition-colors"
                  onClick={() => goToDoc('quickstart')}>
                  <span className="w-1.5 h-1.5 rounded-full bg-primary animate-pulse"></span>v1.1.7 Polyglot release is live
                </div>
                <h1 className="text-5xl md:text-7xl font-bold fade-in-up delay-100 tracking-tight leading-tight select-none">
                  <span className="g-blue inline-block hover:-translate-y-2 transition-transform">P</span>
                  <span className="g-red inline-block hover:-translate-y-2 transition-transform delay-75">i</span>
                  <span className="g-yellow inline-block hover:-translate-y-2 transition-transform delay-100">p</span>
                  <span className="g-blue inline-block hover:-translate-y-2 transition-transform delay-150">e</span>
                  <span className="g-green inline-block hover:-translate-y-2 transition-transform delay-200">Q</span>
                  <span className="g-red inline-block hover:-translate-y-2 transition-transform delay-300">L</span>
                </h1>
                <p className="text-base md:text-lg text-on-surface-variant max-w-xl fade-in-up delay-200 leading-relaxed">
                  Write database queries as clean pipelines. Get injection-safe SQL for Postgres, SQLite, DuckDB, and MySQL.
                </p>
                <form onSubmit={handleSearchSubmit} className="w-full max-w-lg mt-4 fade-in-up delay-300 relative z-20">
                  <div className="absolute inset-y-0 left-0 pl-4 flex items-center pointer-events-none">
                    <span className="material-symbols-outlined text-outline text-lg">search</span>
                  </div>
                  <input className="w-full pl-10 pr-12 py-3 rounded-2xl border border-outline-variant bg-surface-container-lowest shadow-sm focus:ring-2 focus:ring-primary transition-all text-sm text-on-surface placeholder-outline outline-none"
                    placeholder="Search docs..." value={searchValue} onChange={e => setSearchValue(e.target.value)} type="text" />
                  <button type="submit" className="absolute inset-y-0 right-0 pr-3 flex items-center text-outline hover:text-primary transition-colors">
                    <span className="material-symbols-outlined text-lg">arrow_forward</span>
                  </button>
                </form>
                <div className="flex gap-2 mt-4 fade-in-up delay-400">
                  <button className="bg-surface-container-low text-on-surface px-5 py-2 rounded-2xl font-semibold hover:bg-surface-container transition-all border border-outline-variant/30 text-xs" onClick={() => setActiveTab('features')}>Explore Features</button>
                  <button className="bg-primary text-on-primary px-5 py-2 rounded-2xl font-semibold hover:bg-primary/95 transition-all text-xs" onClick={() => goToDoc('intro')}>Read Docs</button>
                  <button className="bg-surface-container-low text-on-surface px-5 py-2 rounded-2xl font-semibold hover:bg-surface-container transition-all border border-outline-variant/30 text-xs" onClick={() => setActiveTab('about')}>About</button>
                </div>
              </div>
            </section>

            {/* Playground — Blocky Fixed-Size WASM Sandbox */}
            <section className="w-full max-w-container-max mx-auto px-4 md:px-6 py-8">
              <div className="flex flex-col md:flex-row md:items-center justify-between mb-4 gap-3">
                <div className="flex items-center gap-3">
                  <h2 className="text-xl font-bold uppercase tracking-wider text-on-surface">Playground</h2>
                  <span className="text-[10px] font-mono font-bold bg-primary/10 text-primary px-2 py-0.5 border border-primary/20">FIXED-SIZE WORKSTATION</span>
                </div>

                {/* Sample Quick Selector */}
                <div className="flex items-center gap-1 overflow-x-auto pb-1 md:pb-0 font-mono text-xs">
                  <span className="text-[10px] font-bold uppercase text-on-surface-variant mr-1 shrink-0">[ SAMPLES ]</span>
                  {Object.keys(SAMPLE_QUERIES).map(k => (
                    <button
                      key={k}
                      onClick={() => loadSample(k)}
                      className="px-2.5 py-1 text-[11px] font-mono text-on-surface-variant hover:text-on-surface bg-surface-container/80 hover:bg-surface-container border border-outline-variant/30 transition-colors whitespace-nowrap"
                    >
                      {SAMPLE_QUERIES[k].title}
                    </button>
                  ))}
                </div>
              </div>

              {/* Main Blocky Box — Fixed Height (500px) */}
              <div className="bg-[#0b0c12] border-2 border-white/20 shadow-2xl h-[500px] flex flex-col">
                {/* Control Bar */}
                <div className="px-4 py-2.5 bg-[#12131d] border-b-2 border-white/10 flex flex-wrap items-center justify-between gap-2 shrink-0">
                  {/* Left: Blocky Dialect Selector */}
                  <div className="flex items-center gap-1 font-mono text-xs">
                    <span className="text-[10px] font-bold text-white/40 uppercase mr-1">DIALECT:</span>
                    {[
                      { id: 'postgres', label: 'POSTGRES' },
                      { id: 'sqlite', label: 'SQLITE' },
                      { id: 'duckdb', label: 'DUCKDB' },
                      { id: 'mysql', label: 'MYSQL' }
                    ].map(d => (
                      <button
                        key={d.id}
                        onClick={() => setCompilerDialect(d.id)}
                        className={`px-3 py-1 text-xs font-mono font-bold border transition-all ${
                          compilerDialect === d.id
                            ? 'bg-[#38bdf8] text-[#090a0f] border-[#38bdf8]'
                            : 'bg-white/5 text-white/60 border-white/10 hover:text-white hover:bg-white/10'
                        }`}
                      >
                        {d.label}
                      </button>
                    ))}
                  </div>

                  {/* Right: Engine Status & Blocky Run Button */}
                  <div className="flex items-center gap-3">
                    <span className="text-[11px] font-mono text-white/50 hidden sm:inline">
                      {wasmReady ? (
                        <span className="text-emerald-400 font-bold">
                          ● WASM READY {compileTime && `[ ${compileTime}ms ]`}
                        </span>
                      ) : (
                        'COMPILER INITIALIZING...'
                      )}
                    </span>

                    <button
                      onClick={() => handleRun()}
                      disabled={!wasmReady || isCompiling}
                      className="flex items-center gap-2 px-5 py-1.5 bg-[#38bdf8] hover:bg-[#38bdf8]/90 text-[#090a0f] font-mono font-bold text-xs border border-[#38bdf8] transition-all active:translate-y-0.5 disabled:opacity-50 cursor-pointer"
                    >
                      <Play size={12} className="fill-current" />
                      <span>{isCompiling ? 'COMPILING...' : 'RUN QUERY'}</span>
                      <kbd className="hidden sm:inline-block text-[9px] bg-black/20 px-1 py-0.5 font-mono text-[#090a0f] border border-black/10">CTRL+ENTER</kbd>
                    </button>
                  </div>
                </div>

                {/* Split Editor Grid — Fixed Height Flex-1 */}
                <div className="grid grid-cols-1 lg:grid-cols-2 flex-1 min-h-0 divide-y lg:divide-y-0 lg:divide-x-2 divide-white/10">
                  {/* PipeQL Input */}
                  <div className="flex flex-col h-full bg-[#08090e]">
                    <div className="px-4 py-2 bg-[#10111a] border-b border-white/10 flex items-center justify-between text-xs font-mono shrink-0">
                      <span className="text-[11px] font-bold text-white/80 uppercase">INPUT // query.pipeql</span>
                      <button onClick={() => setEditorValue('')} className="text-white/40 hover:text-white uppercase text-[10px]">CLEAR</button>
                    </div>
                    <textarea
                      ref={textareaRef}
                      value={editorValue}
                      onChange={e => setEditorValue(e.target.value)}
                      onKeyDown={handleKeyDown}
                      spellCheck={false}
                      className="w-full flex-1 h-full p-4 bg-transparent font-mono text-xs md:text-[13px] text-white/90 leading-relaxed resize-none focus:outline-none placeholder-white/20 border-none overflow-y-auto"
                      placeholder="Write PipeQL query here..."
                    />
                  </div>

                  {/* Output Viewer */}
                  <div className="flex flex-col h-full bg-[#08090e]">
                    <div className="px-4 py-2 bg-[#10111a] border-b border-white/10 flex items-center justify-between text-xs font-mono shrink-0">
                      <div className="flex items-center gap-1">
                        {[
                          { id: 'sql', label: 'COMPILED SQL' },
                          { id: 'params', label: `PARAMS (${compileResult?.params?.length || 0})` },
                          { id: 'ast', label: 'AST ANALYSIS' }
                        ].map(t => (
                          <button
                            key={t.id}
                            onClick={() => setOutputTab(t.id)}
                            className={`px-3 py-1 text-[11px] font-bold border transition-all ${
                              outputTab === t.id
                                ? 'bg-white/15 text-white border-white/30'
                                : 'text-white/40 border-transparent hover:text-white/80'
                            }`}
                          >
                            {t.label}
                          </button>
                        ))}
                      </div>

                      <button
                        onClick={() => compileResult && copyToClipboard(compileResult.sql)}
                        className="text-[10px] font-mono text-white/60 hover:text-white transition-colors flex items-center gap-1 border border-white/10 px-2 py-0.5 bg-white/5"
                      >
                        {copiedCode ? <Check size={11} className="text-emerald-400" /> : <Copy size={11} />}
                        {copiedCode ? 'COPIED' : 'COPY'}
                      </button>
                    </div>

                    {/* Output Panel Content — Scrollable inside fixed container */}
                    <div className="flex-1 p-4 overflow-y-auto font-mono text-xs">
                      {compileError ? (
                        <div className="p-3 bg-red-500/10 border-2 border-red-500/40 text-red-400 leading-relaxed whitespace-pre-wrap">
                          <span className="font-bold block mb-1 uppercase tracking-wider text-[11px]">ERROR:</span>
                          {compileError}
                        </div>
                      ) : compileResult ? (
                        <>
                          {outputTab === 'sql' && (
                            <div className="space-y-3">
                              <div className="text-[10px] text-white/40 uppercase tracking-widest border-b border-white/5 pb-1">
                                TARGET: {compilerDialect.toUpperCase()} // PARAMS: {compileResult.params?.length || 0}
                              </div>
                              <pre className="text-emerald-400 whitespace-pre-wrap leading-relaxed font-mono">{compileResult.sql}</pre>
                            </div>
                          )}

                          {outputTab === 'params' && (
                            <div className="space-y-2">
                              <div className="text-[10px] text-white/40 uppercase mb-2 flex items-center justify-between">
                                <span>EXTRACTED BIND PARAMETERS</span>
                                <span className="text-emerald-400 border border-emerald-400/30 px-1.5 py-0.5">100% ISOLATED</span>
                              </div>
                              {compileResult.params?.length > 0 ? (
                                <div className="border border-white/15 divide-y divide-white/10 bg-white/[0.02]">
                                  {compileResult.params.map((val, idx) => (
                                    <div key={idx} className="p-2 flex items-center justify-between text-xs">
                                      <span className="text-sky-400 font-bold">${idx + 1}</span>
                                      <span className="text-white/80 bg-white/5 px-2 py-0.5 border border-white/10">"{val}"</span>
                                    </div>
                                  ))}
                                </div>
                              ) : (
                                <div className="text-white/30 py-8 text-center text-xs">NO DYNAMIC PARAMETERS IN THIS STATEMENT</div>
                              )}
                            </div>
                          )}

                          {outputTab === 'ast' && (
                            <div className="space-y-3">
                              <div className="flex gap-2">
                                <span className="px-2 py-1 bg-white/5 border border-white/10 text-white/70 text-[10px] uppercase">
                                  TYPE: <strong className="text-sky-400">{compileResult.statementType || 'QUERY'}</strong>
                                </span>
                                <span className="px-2 py-1 bg-white/5 border border-white/10 text-white/70 text-[10px] uppercase">
                                  MUTATION: <strong className={compileResult.isMutation ? 'text-red-400' : 'text-emerald-400'}>{compileResult.isMutation ? 'YES' : 'NO'}</strong>
                                </span>
                              </div>
                              {compileResult.analysis && (
                                <pre className="text-white/70 p-3 bg-white/[0.02] border border-white/10 overflow-x-auto text-[11px]">{JSON.stringify(compileResult.analysis, null, 2)}</pre>
                              )}
                            </div>
                          )}
                        </>
                      ) : (
                        <div className="flex items-center justify-center h-full text-white/30 text-xs uppercase tracking-wider">
                          READY TO COMPILE
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              </div>
            </section>
          </div>
        )}

        {/* ═══════════════════════════════════════════════════════════ */}
        {/* ABOUT                                                     */}
        {/* ═══════════════════════════════════════════════════════════ */}
        {activeTab === 'about' && (
          <div className="w-full overflow-y-auto" style={{scrollbarWidth:'none'}}>
            <div className="max-w-3xl mx-auto px-6 py-12">
              <div className="mb-10">
                <h1 className="text-4xl font-bold text-on-surface mb-4">About PipeQL</h1>
                <p className="text-on-surface-variant text-sm">Built by <strong>Flaxmbot</strong> for developers who are tired of writing unsafe SQL.</p>
              </div>

              <div className="space-y-8">
                <div className="space-y-3">
                  <SectionTitle>The Problem</SectionTitle>
                  <p className="text-on-surface-variant text-sm leading-relaxed">Every day, developers write database queries by concatenating strings. It's fast to write, easy to forget about, and dangerous.</p>
                  <CodeBlock label="the problem">{`// This looks fine. It's not.
const query = "SELECT * FROM users WHERE name = '" + userName + "'";

// If userName = "'; DROP TABLE users; --"
// Your database is gone.`}</CodeBlock>
                  <p className="text-on-surface-variant text-sm leading-relaxed">ORMs were supposed to fix this. Instead they added a layer of magic that hides what's actually happening, slows things down, and still doesn't fully prevent injection in every case.</p>
                </div>

                <div className="space-y-3">
                  <SectionTitle>What PipeQL Does</SectionTitle>
                  <p className="text-on-surface-variant text-sm leading-relaxed">PipeQL is a query language that compiles to SQL. You write queries using a clean pipe syntax. The compiler extracts every value into bind parameters automatically. The generated SQL never contains user input — it's mathematically impossible to inject.</p>
                  <CodeBlock label="the solution">{`// You write this:
from users | filter name == $name and age >= $min

// PipeQL compiles to this:
SELECT * FROM users WHERE (name = $1) AND (age >= $2)
// params: ["name", "min"]

// The string "'; DROP TABLE users; --" becomes
// a safe parameter value, never SQL text.`}</CodeBlock>
                </div>

                <div className="space-y-3">
                  <SectionTitle>Why a New Language?</SectionTitle>
                  <p className="text-on-surface-variant text-sm leading-relaxed">Existing query builders have two modes: raw SQL (dangerous) or ORMs (slow, leaky abstraction). PipeQL is a third option — a language that compiles to SQL the way TypeScript compiles to JavaScript. You get safety, readability, and performance.</p>
                </div>

                <div className="space-y-3">
                  <SectionTitle>Who Built This</SectionTitle>
                  <p className="text-on-surface-variant text-sm leading-relaxed">PipeQL is built by <strong>Flaxmbot</strong>, a developer tools studio focused on making database interactions safe by default. The core compiler is written in Rust with <code>#![deny(unsafe_code)]</code> — no memory bugs, no unsafe blocks, no exceptions.</p>
                </div>

                <div className="space-y-3">
                  <SectionTitle>Key Numbers</SectionTitle>
                  <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                    {[
                      ['~19µs', 'Compile time'],
                      ['4', 'SQL dialects'],
                      ['5', 'Language SDKs'],
                      ['100%', 'Parameter isolation']
                    ].map(([num, label]) => (
                      <div key={label} className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl p-4 text-center">
                        <div className="text-xl font-bold text-primary">{num}</div>
                        <div className="text-[10px] text-on-surface-variant mt-0.5">{label}</div>
                      </div>
                    ))}
                  </div>
                </div>

                <div className="space-y-3">
                  <SectionTitle>Open Source</SectionTitle>
                  <p className="text-on-surface-variant text-sm leading-relaxed">PipeQL is MIT licensed. The entire compiler, all SDKs, the LSP, VS Code extension, and tree-sitter grammar are open source on GitHub.</p>
                  <a href="https://github.com/Flaxmbot/PipeQL" target="_blank" rel="noreferrer"
                    className="inline-flex items-center gap-2 bg-surface-container-lowest border border-outline-variant/20 rounded-2xl px-5 py-2.5 text-sm font-semibold text-on-surface hover:bg-surface-container transition-all">
                    <span className="material-symbols-outlined text-base">terminal</span> View on GitHub
                  </a>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* ═══════════════════════════════════════════════════════════ */}
        {/* FEATURES (redesigned)                                     */}
        {/* ═══════════════════════════════════════════════════════════ */}
        {activeTab === 'features' && (
          <div className="w-full overflow-y-auto" style={{scrollbarWidth:'none'}}>
            <div className="max-w-4xl mx-auto px-6 py-12">
              <div className="text-center mb-12">
                <h1 className="text-4xl font-bold tracking-tight text-on-surface mb-3">Why PipeQL?</h1>
                <p className="text-on-surface-variant max-w-xl mx-auto text-sm">One language. Four databases. Zero injection risk. Faster than raw SQL.</p>
              </div>

              {/* Top 3 features */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-5 mb-12">
                {[
                  { color: '#4285F4', icon: 'bolt', title: '19µs Compile', desc: 'Hand-written Rust parser. Faster than any ORM, faster than string concatenation.' },
                  { color: '#EA4335', icon: 'shield', title: 'Injection Proof', desc: 'Values never enter SQL text. Extracted to bind params at the AST level. Mathematically proven.' },
                  { color: '#FBBC05', icon: 'swap_horiz', title: '4 Dialects', desc: 'Write once. Compile to PostgreSQL, SQLite, DuckDB, or MySQL. Swap databases without rewriting queries.' }
                ].map((f,i) => (
                  <div key={i} className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl p-5 relative overflow-hidden group hover:shadow-lg transition-all">
                    <div className="absolute top-0 left-0 w-full h-0.5" style={{background:f.color}}></div>
                    <div className="w-10 h-10 rounded-xl bg-surface-container flex items-center justify-center mb-3">
                      <span className="material-symbols-outlined text-xl" style={{color:f.color}}>{f.icon}</span>
                    </div>
                    <h3 className="text-sm font-bold mb-1.5 text-on-surface">{f.title}</h3>
                    <p className="text-on-surface-variant text-xs leading-relaxed">{f.desc}</p>
                  </div>
                ))}
              </div>

              {/* Comparison table */}
              <div className="space-y-4 mb-12">
                <SectionTitle>PipeQL vs Alternatives</SectionTitle>
                <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                  <table className="w-full text-left text-sm bg-surface-container-lowest">
                    <thead><tr className="bg-surface border-b border-outline-variant/20 text-[10px] font-bold uppercase tracking-wider text-on-surface">
                      <th className="px-4 py-2.5">Feature</th><th className="px-4 py-2.5">Raw SQL</th><th className="px-4 py-2.5">ORMs</th><th className="px-4 py-2.5 text-primary">PipeQL</th>
                    </tr></thead>
                    <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant text-xs">
                      <tr><td className="px-4 py-2.5 font-semibold">SQL Injection Risk</td><td className="px-4 py-2.5">High</td><td className="px-4 py-2.5">Low</td><td className="px-4 py-2.5 font-bold text-g-green">None</td></tr>
                      <tr><td className="px-4 py-2.5 font-semibold">Compile Speed</td><td className="px-4 py-2.5">N/A</td><td className="px-4 py-2.5">~1ms</td><td className="px-4 py-2.5 font-bold text-g-green">~19µs</td></tr>
                      <tr><td className="px-4 py-2.5 font-semibold">Multi-dialect</td><td className="px-4 py-2.5">Manual</td><td className="px-4 py-2.5">Partial</td><td className="px-4 py-2.5 font-bold text-g-green">4 dialects</td></tr>
                      <tr><td className="px-4 py-2.5 font-semibold">Readability</td><td className="px-4 py-2.5">Low</td><td className="px-4 py-2.5">Medium</td><td className="px-4 py-2.5 font-bold text-g-green">High</td></tr>
                      <tr><td className="px-4 py-2.5 font-semibold">Learning Curve</td><td className="px-4 py-2.5">None</td><td className="px-4 py-2.5">High</td><td className="px-4 py-2.5 font-bold text-g-green">Low</td></tr>
                    </tbody>
                  </table>
                </div>
              </div>

              {/* Dialect mapping */}
              <div className="space-y-4">
                <SectionTitle>Type Mapping Across Dialects</SectionTitle>
                <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                  <table className="w-full text-left text-sm bg-surface-container-lowest">
                    <thead><tr className="bg-surface border-b border-outline-variant/20 text-[10px] font-bold uppercase tracking-wider text-on-surface">
                      <th className="px-4 py-2.5">PipeQL Type</th><th className="px-4 py-2.5">PostgreSQL</th><th className="px-4 py-2.5">SQLite</th><th className="px-4 py-2.5">DuckDB</th><th className="px-4 py-2.5">MySQL</th>
                    </tr></thead>
                    <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant">
                      {[
                        ['int / integer','INTEGER','INTEGER','INTEGER','INT'],
                        ['float / real','DOUBLE PRECISION','REAL','DOUBLE','DOUBLE'],
                        ['string / text','TEXT','TEXT','VARCHAR','VARCHAR(255)'],
                        ['bool / boolean','BOOLEAN','INTEGER','BOOLEAN','BOOLEAN'],
                        ['timestamp / datetime','TIMESTAMP','DATETIME','TIMESTAMP','TIMESTAMP']
                      ].map(([t,...ds]) => (
                        <tr key={t}><td className="px-4 py-2.5 font-semibold text-primary font-mono text-xs">{t}</td>{ds.map((d,i) => <td key={i} className="px-4 py-2.5">{d}</td>)}</tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* ═══════════════════════════════════════════════════════════ */}
        {/* DOCS (with sidebar)                                       */}
        {/* ═══════════════════════════════════════════════════════════ */}
        {activeTab === 'docs' && (
          <div className="flex flex-1 w-full max-w-container-max mx-auto overflow-hidden">
            <Sidebar />
            <main className="flex-1 py-8 px-3 md:px-6 max-w-4xl h-[calc(100vh-52px)] overflow-y-auto" style={{scrollbarWidth:'none'}}>
              <nav className="flex text-on-surface-variant text-[10px] font-semibold mb-4">
                <ol className="inline-flex items-center space-x-1">
                  <li><a className="hover:text-primary cursor-pointer" onClick={() => setActiveTab('home')}>Home</a></li>
                  <li><span className="material-symbols-outlined text-xs">chevron_right</span></li>
                  <li className="text-on-surface font-semibold capitalize">{activeDocSection.replace(/-/g, ' ')}</li>
                </ol>
              </nav>

              {/* ── INTRODUCTION ── */}
              {activeDocSection === 'intro' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">What is PipeQL?</h1>
                  <p className="text-on-surface-variant leading-relaxed">PipeQL is a query language that compiles to SQL. You write queries using a clean left-to-right pipe syntax. The compiler outputs safe, parameterized SQL for PostgreSQL, SQLite, DuckDB, and MySQL.</p>
                  <p className="text-on-surface-variant text-sm leading-relaxed">Think of it like TypeScript for SQL. You write in a cleaner language, and PipeQL compiles it to the real thing — except every value is automatically extracted into a bind parameter, making SQL injection mathematically impossible.</p>
                  <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 mt-4">
                    {[
                      ['bolt','bg-primary-fixed/40','~19µs Compile','Hand-written Rust parser, faster than any ORM.'],
                      ['shield','bg-[#EA4335]/10','Injection Proof','Values never touch SQL text. Extracted at parse time.'],
                      ['swap_horiz','bg-[#FBBC05]/10','4 Dialects','One query, four databases. Postgres, SQLite, DuckDB, MySQL.'],
                      ['code','bg-[#34A853]/10','5 SDKs','Rust, JavaScript, Python, C, Go.']
                    ].map(([icon,bg,title,desc]) => (
                      <div key={title} className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl p-4">
                        <div className={`w-9 h-9 ${bg} rounded-xl flex items-center justify-center mb-2`}>
                          <span className="material-symbols-outlined text-lg">{icon}</span>
                        </div>
                        <h4 className="text-xs font-bold text-on-surface mb-1">{title}</h4>
                        <p className="text-[11px] text-on-surface-variant">{desc}</p>
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* ── QUICK START ── */}
              {activeDocSection === 'quickstart' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Quick Start</h1>
                  <p className="text-on-surface-variant text-sm">Get running in 3 steps.</p>
                  <StepCard num={1} title="Install">
                    <div className="space-y-2 mt-2">
                      {[
                        ['Node.js (WASM)', 'npm install @flaxmbot/pipeql'],
                        ['Python (PyO3)', 'pip install pipeql'],
                        ['C/C++ (CFFI)', 'cargo build --release -p pipeql-cffi'],
                        ['Go (CGO)', 'go get github.com/Flaxmbot/PipeQL/go@latest'],
                        ['CLI', 'cargo install pipeql-cli']
                      ].map(([lang, cmd]) => (
                        <CodeBlock key={lang} label={lang}>{cmd}</CodeBlock>
                      ))}
                    </div>
                  </StepCard>
                  <StepCard num={2} title="Setup Driver">
                    <CodeBlock label="db.js">{`import sqlite3 from 'sqlite3';
import { createPipeqlDriver } from '@flaxmbot/pipeql/driver';

const db = createPipeqlDriver(new sqlite3.Database('app.db'), { dialect: 'sqlite' });`}</CodeBlock>
                  </StepCard>
                  <StepCard num={3} title="Run a Query">
                    <CodeBlock label="query.js">{`const users = await db.query(
  'from users | filter role == $role',
  { role: 'admin' }
);`}</CodeBlock>
                  </StepCard>
                </div>
              )}

              {/* ── TUTORIAL 1: First Query ── */}
              {activeDocSection === 'tutorial-1' && (
                <div className="space-y-5">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="bg-primary text-on-primary text-[9px] font-bold px-2 py-0.5 rounded-lg">TUTORIAL</span>
                    <span className="text-[10px] text-outline">Step 1 of 5</span>
                  </div>
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Your First PipeQL Query</h1>
                  <p className="text-on-surface-variant text-sm leading-relaxed">PipeQL reads like English. Data flows from left to right through pipes (<InlineCode>|</InlineCode>).</p>

                  <SectionTitle>Reading Data</SectionTitle>
                  <p className="text-sm text-on-surface-variant">The <InlineCode>from</InlineCode> keyword starts every read query. It's like <InlineCode>SELECT * FROM</InlineCode> in SQL.</p>
                  <CodeBlock label="PipeQL">{`from users`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">Compiles to: <InlineCode>SELECT * FROM users;</InlineCode></p>

                  <SectionTitle>Adding a Filter</SectionTitle>
                  <p className="text-sm text-on-surface-variant">Use <InlineCode>filter</InlineCode> to narrow results. It's like <InlineCode>WHERE</InlineCode>.</p>
                  <CodeBlock label="PipeQL">{`from users
| filter age >= 18`}</CodeBlock>
                    <p className="text-xs text-on-surface-variant">Compiles to: <InlineCode>{'SELECT * FROM users WHERE (age >= 18);'}</InlineCode></p>

                  <SectionTitle>Limiting Results</SectionTitle>
                  <p className="text-sm text-on-surface-variant">Use <InlineCode>take</InlineCode> to limit rows. It's like <InlineCode>LIMIT</InlineCode>.</p>
                  <CodeBlock label="PipeQL">{`from users
| filter age >= 18
| take 10`}</CodeBlock>
                    <p className="text-xs text-on-surface-variant">Compiles to: <InlineCode>{'SELECT * FROM users WHERE (age >= 18) LIMIT 10;'}</InlineCode></p>

                  <SectionTitle>Selecting Columns</SectionTitle>
                  <p className="text-sm text-on-surface-variant">Use <InlineCode>select</InlineCode> to pick specific columns.</p>
                  <CodeBlock label="PipeQL">{`from users
| filter age >= 18
| select [id, name, email]
| take 10`}</CodeBlock>
                    <p className="text-xs text-on-surface-variant">Compiles to: <InlineCode>{'SELECT id, name, email FROM users WHERE (age >= 18) LIMIT 10;'}</InlineCode></p>

                  <Warning type="info">Every value you type directly (like <InlineCode>18</InlineCode>) is still extracted into a bind parameter. Even constants are safe.</Warning>
                </div>
              )}

              {/* ── TUTORIAL 2: Filters & Parameters ── */}
              {activeDocSection === 'tutorial-2' && (
                <div className="space-y-5">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="bg-primary text-on-primary text-[9px] font-bold px-2 py-0.5 rounded-lg">TUTORIAL</span>
                    <span className="text-[10px] text-outline">Step 2 of 5</span>
                  </div>
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Filters & Parameters</h1>
                  <p className="text-on-surface-variant text-sm">Parameters make queries dynamic and safe. Use <InlineCode>$name</InlineCode> or <InlineCode>${'{name}'}</InlineCode> syntax.</p>

                  <SectionTitle>Using Parameters</SectionTitle>
                  <CodeBlock label="PipeQL">{`from users
| filter role == $role and age >= $min_age`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">When you call <InlineCode>compile(source, dialect)</InlineCode>, you get the SQL and a params array. The database driver binds these values safely.</p>
                  <CodeBlock label="Result">{`SQL:    SELECT * FROM users WHERE (role = $1) AND (age >= $2);
Params: ["role", "min_age"]`}</CodeBlock>

                  <SectionTitle>Comparison Operators</SectionTitle>
                  <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                    <table className="w-full text-left text-xs bg-surface-container-lowest">
                      <thead><tr className="bg-surface border-b border-outline-variant/20 text-[9px] font-bold uppercase tracking-wider"><th className="px-3 py-2">Operator</th><th className="px-3 py-2">Meaning</th><th className="px-3 py-2">Example</th></tr></thead>
                      <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant">
                        <tr><td className="px-3 py-2 font-mono text-primary">==</td><td className="px-3 py-2">Equals</td><td className="px-3 py-2 font-mono">name == $name</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">!=</td><td className="px-3 py-2">Not equals</td><td className="px-3 py-2 font-mono">status != 'deleted'</td></tr>
                          <tr><td className="px-3 py-2 font-mono text-primary">&gt;=</td><td className="px-3 py-2">Greater or equal</td><td className="px-3 py-2 font-mono">{'age >= $min'}</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">&lt;=</td><td className="px-3 py-2">Less or equal</td><td className="px-3 py-2 font-mono">price &lt;= $max</td></tr>
                      </tbody>
                    </table>
                  </div>

                  <SectionTitle>Combining Filters</SectionTitle>
                  <CodeBlock label="PipeQL">{`from products
| filter category == $cat and price >= $min and price <= $max
| sort [price asc]
| take 20`}</CodeBlock>
                </div>
              )}

              {/* ── TUTORIAL 3: Joins & Groups ── */}
              {activeDocSection === 'tutorial-3' && (
                <div className="space-y-5">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="bg-primary text-on-primary text-[9px] font-bold px-2 py-0.5 rounded-lg">TUTORIAL</span>
                    <span className="text-[10px] text-outline">Step 3 of 5</span>
                  </div>
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Joins & Groups</h1>
                  <p className="text-on-surface-variant text-sm">Combine tables and aggregate data with <InlineCode>join</InlineCode> and <InlineCode>group</InlineCode>.</p>

                  <SectionTitle>Joining Tables</SectionTitle>
                  <CodeBlock label="PipeQL">{`from orders
| join customers on orders.customer_id == customers.id
| select [orders.id, customers.name, orders.total]`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">Compiles to: <InlineCode>SELECT ... FROM orders INNER JOIN customers ON (orders.customer_id = customers.id);</InlineCode></p>

                  <SectionTitle>Grouping & Aggregation</SectionTitle>
                  <CodeBlock label="PipeQL">{`from orders
| join customers on orders.customer_id == customers.id
| group [customers.region] (
    total = sum(orders.total),
    order_count = count(*)
  )
| sort [total desc]
| take 10`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">Available aggregates: <InlineCode>sum()</InlineCode>, <InlineCode>count()</InlineCode>, <InlineCode>min()</InlineCode>, <InlineCode>max()</InlineCode>, <InlineCode>avg()</InlineCode></p>

                  <SectionTitle>Sorting & Pagination</SectionTitle>
                  <CodeBlock label="PipeQL">{`from products
| filter status == 'active'
| sort [price asc, name asc]
| skip 20
| take 10`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant"><InlineCode>skip</InlineCode> = OFFSET, <InlineCode>take</InlineCode> = LIMIT. Use together for pagination.</p>
                </div>
              )}

              {/* ── TUTORIAL 4: Writing Data ── */}
              {activeDocSection === 'tutorial-4' && (
                <div className="space-y-5">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="bg-primary text-on-primary text-[9px] font-bold px-2 py-0.5 rounded-lg">TUTORIAL</span>
                    <span className="text-[10px] text-outline">Step 4 of 5</span>
                  </div>
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Writing Data</h1>
                  <p className="text-on-surface-variant text-sm">Insert, update, and delete records. All values are automatically parameterized.</p>

                  <SectionTitle>Insert</SectionTitle>
                  <CodeBlock label="PipeQL">{`into users
| insert [
    name = $name,
    email = $email,
    role = 'user'
  ]`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">PostgreSQL adds <InlineCode>RETURNING *</InlineCode> automatically. SQLite/MySQL return execution metadata.</p>

                  <SectionTitle>Update</SectionTitle>
                  <CodeBlock label="PipeQL">{`from users
| filter id == $id
| update [
    name = $new_name,
    updated_at = current_timestamp
  ]`}</CodeBlock>
                  <Warning>Update requires a filter. You cannot update without specifying which rows.</Warning>

                  <SectionTitle>Delete</SectionTitle>
                  <CodeBlock label="PipeQL">{`from users
| filter id == $id
| delete`}</CodeBlock>
                  <Warning>Delete also requires a filter. The compiler rejects <code>from users | delete</code> (would delete all rows).</Warning>
                </div>
              )}

              {/* ── TUTORIAL 5: Real App ── */}
              {activeDocSection === 'tutorial-5' && (
                <div className="space-y-5">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="bg-primary text-on-primary text-[9px] font-bold px-2 py-0.5 rounded-lg">TUTORIAL</span>
                    <span className="text-[10px] text-outline">Step 5 of 5</span>
                  </div>
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Building a Real App</h1>
                  <p className="text-on-surface-variant text-sm">Complete CRUD API using PipeQL driver adapters.</p>

                  <SectionTitle>Step 1: Create the Table</SectionTitle>
                  <CodeBlock label="schema.pql">{`table users [
  id int primary auto,
  name string not null,
  email string not null unique,
  role string default 'user',
  created_at timestamp default current_timestamp
]`}</CodeBlock>

                  <SectionTitle>Step 2: Create the Driver</SectionTitle>
                  <CodeBlock label="db.js">{`import sqlite3 from 'sqlite3';
import { createPipeqlDriver } from '@flaxmbot/pipeql/driver';

export const db = createPipeqlDriver(
  new sqlite3.Database('app.db'),
  { dialect: 'sqlite' }
);`}</CodeBlock>

                  <SectionTitle>Step 3: CRUD Operations</SectionTitle>
                  <CodeBlock label="routes.js">{`// CREATE
const newUser = await db.insertAndFetch(
  'into users | insert $data',
  { name: req.body.name, email: req.body.email }
);

// READ
const users = await db.query(
  'from users | filter role == $role | sort [name asc]',
  { role: 'admin' }
);

// UPDATE
const updated = await db.updateAndFetch(
  'from users | filter id == $id | update $data',
  { id: req.params.id, data: { name: 'New Name' } }
);

// DELETE
await db.execute('from users | filter id == $id | delete', { id: req.params.id });`}</CodeBlock>

                  <SectionTitle>Step 4: Express Server</SectionTitle>
                  <CodeBlock label="server.js">{`import express from 'express';
import { db } from './db.js';

const app = express();
app.use(express.json());

app.get('/users', async (req, res) => {
  const users = await db.query('from users | sort [name asc]');
  res.json(users);
});

app.post('/users', async (req, res) => {
  const user = await db.insertAndFetch('into users | insert $data', req.body);
  res.json(user);
});

app.listen(3000);`}</CodeBlock>
                  <p className="text-xs text-on-surface-variant">That's it. No raw SQL strings anywhere. Every database interaction goes through PipeQL.</p>
                </div>
              )}

              {/* ── SYNTAX ── */}
              {activeDocSection === 'syntax' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Query Syntax</h1>
                  <SectionTitle>EBNF Grammar</SectionTitle>
                  <CodeBlock label="grammar">{`statement   ::= NEWLINE* (pipeline | insert_stmt | upsert_stmt | delete_stmt | table_stmt) EOF
pipeline    ::= source (SEP step)*
source      ::= 'from' IDENT
step        ::= filter_step | select_step | join_step | group_step
           |   sort_step | take_step | skip_step | update_step
           |   union_step
filter_step ::= 'filter' expression
select_step ::= 'select' '[' (select_item (',' select_item)*)? ']'
join_step   ::= 'join' IDENT 'on' expression
group_step  ::= 'group' '[' columns ']' '(' aggregates ')'
sort_step   ::= 'sort' '[' sort_item (',' sort_item)* ']'
take_step   ::= 'take' INT
skip_step   ::= 'skip' INT
update_step ::= 'update' '[' (assignment (',' assignment)*)? ']'
delete_step ::= 'delete'
union_step  ::= 'union' 'all'?
insert_stmt ::= 'into' IDENT '| insert' '[' assignments ']'
upsert_stmt ::= 'into' IDENT '| upsert' '[' assignments ']'
              'conflict' '[' columns ']'
              'do update' '[' assignments ']'
table_stmt  ::= 'table' IDENT '[' column_defs ']'

expression  ::= atom compare_op atom | atom ('and'|'or') atom
           |   atom 'in' '(' pipeline ')'
compare_op  ::= '==' | '!=' | '>=' | '<=' | '>' | '<'
param_ref   ::= '$' IDENT | '${' IDENT '}'
func_call   ::= IDENT '(' args ')'`}</CodeBlock>
                  <Warning type="info"><strong>Terminal Rule:</strong> Update and delete must be the last step.</Warning>
                </div>
              )}

              {/* ── MUTATIONS ── */}
              {activeDocSection === 'mutations' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Mutations (DML)</h1>
                  <div className="space-y-3">
                    <SectionTitle>INSERT</SectionTitle>
                    <CodeBlock label="PipeQL">{`into notes | insert [title = $title, content = $content, category = 'Personal']`}</CodeBlock>
                    <div className="grid grid-cols-2 gap-3">
                      <CodeBlock label="PostgreSQL">{`INSERT INTO notes (title, content, category)
VALUES ($1, $2, $3) RETURNING *;`}</CodeBlock>
                      <CodeBlock label="SQLite">{`INSERT INTO notes (title, content, category)
VALUES (?, ?, ?);`}</CodeBlock>
                    </div>
                  </div>
                  <div className="space-y-3">
                    <SectionTitle>UPDATE</SectionTitle>
                    <CodeBlock label="PipeQL">{`from notes | filter id == $id | update [title = $title, updated_at = current_timestamp]`}</CodeBlock>
                    <CodeBlock label="PostgreSQL">{`UPDATE notes SET title = $1, updated_at = CURRENT_TIMESTAMP WHERE (id = $2);`}</CodeBlock>
                    <Warning type="error">
                      <strong>Safety guard:</strong> <InlineCode>update</InlineCode> requires a preceding <InlineCode>filter</InlineCode> stage — PipeQL rejects unfiltered updates to prevent accidental mass updates.
                    </Warning>
                    <SectionTitle>Update Every Row (escape hatch)</SectionTitle>
                    <p className="text-sm text-on-surface-variant">To deliberately update every row, write <InlineCode>update all [...]</InlineCode>. This is the explicit opt-in that bypasses the filter guard.</p>
                    <CodeBlock label="PipeQL">{`from users | update all [plan = $plan]`}</CodeBlock>
                    <CodeBlock label="PostgreSQL">{`UPDATE users SET plan = $1;`}</CodeBlock>
                    <p className="text-xs text-on-surface-variant">If a <InlineCode>filter</InlineCode> is present alongside <InlineCode>all</InlineCode>, the <InlineCode>WHERE</InlineCode> clause still applies.</p>
                  </div>
                  <div className="space-y-3">
                    <SectionTitle>DELETE</SectionTitle>
                    <CodeBlock label="PipeQL">{`from notes | filter id == $id | delete`}</CodeBlock>
                    <CodeBlock label="PostgreSQL">{`DELETE FROM notes WHERE (id = $1);`}</CodeBlock>
                    <Warning type="error">
                      <strong>Safety guard:</strong> <InlineCode>delete</InlineCode> requires a preceding <InlineCode>filter</InlineCode> stage — same enforcement as <InlineCode>update</InlineCode>.
                    </Warning>
                    <SectionTitle>Delete Every Row (escape hatch)</SectionTitle>
                    <p className="text-sm text-on-surface-variant">To deliberately clear a table, write <InlineCode>delete all</InlineCode> — the explicit opt-in that bypasses the filter guard.</p>
                    <CodeBlock label="PipeQL">{`from users | delete all`}</CodeBlock>
                    <CodeBlock label="PostgreSQL">{`DELETE FROM users;`}</CodeBlock>
                    <p className="text-xs text-on-surface-variant">If a <InlineCode>filter</InlineCode> is present alongside <InlineCode>all</InlineCode>, the <InlineCode>WHERE</InlineCode> clause still applies.</p>
                  </div>
                </div>
              )}

              {/* ── UPSERT ── */}
              {activeDocSection === 'upsert' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Upsert (Insert or Update)</h1>
                  <p className="text-on-surface-variant text-sm">Upsert inserts a row and updates it if a conflict occurs. Available since v1.1.</p>
                  <SectionTitle>Syntax</SectionTitle>
                  <CodeBlock label="PipeQL">{`into users
| upsert [name = $name, email = $email]
| conflict [email]
| do update [name = $name]`}</CodeBlock>
                  <SectionTitle>Dialect Output</SectionTitle>
                  <div className="grid grid-cols-2 gap-3">
                    <CodeBlock label="PostgreSQL">{`INSERT INTO users (name, email) VALUES ($1, $2)
ON CONFLICT (email) DO UPDATE SET name = $1
RETURNING *;`}</CodeBlock>
                    <CodeBlock label="SQLite / DuckDB">{`INSERT INTO users (name, email) VALUES (?, ?)
ON CONFLICT (email) DO UPDATE SET name = ?;`}</CodeBlock>
                  </div>
                  <div className="grid grid-cols-1 gap-3">
                    <CodeBlock label="MySQL">{`INSERT INTO users (name, email) VALUES (?, ?)
ON DUPLICATE KEY UPDATE name = ?;`}</CodeBlock>
                  </div>
                  <Warning type="info"><strong>Note:</strong> MySQL uses <InlineCode>ON DUPLICATE KEY UPDATE</InlineCode> instead of <InlineCode>ON CONFLICT</InlineCode> because MySQL doesn't support conflict targets.</Warning>
                </div>
              )}

              {/* ── UNION ── */}
              {activeDocSection === 'union' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Union (Combine Results)</h1>
                  <p className="text-on-surface-variant text-sm">Union combines results from two or more queries. Available since v1.1.</p>
                  <SectionTitle>Syntax</SectionTitle>
                  <CodeBlock label="PipeQL">{`from active_users
| select [id, name]
| union all
from archived_users
| select [id, name]`}</CodeBlock>
                  <SectionTitle>Dialect Output</SectionTitle>
                  <CodeBlock label="All Dialects">{`SELECT id, name FROM active_users
UNION ALL
SELECT id, name FROM archived_users;`}</CodeBlock>
                  <Warning type="info"><strong>Note:</strong> <InlineCode>union all</InlineCode> keeps duplicates. Use <InlineCode>union</InlineCode> (without <InlineCode>all</InlineCode>) for distinct results.</Warning>
                </div>
              )}

              {/* ── SUBQUERY ── */}
              {activeDocSection === 'subquery' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Subqueries</h1>
                  <p className="text-on-surface-variant text-sm">Subqueries let you nest a query inside a filter. Available since v1.1.</p>
                  <SectionTitle>IN Subquery</SectionTitle>
                  <CodeBlock label="PipeQL">{`from orders
| filter customer_id in (
    from customers
    | filter region == 'EU'
    | select [id]
  )`}</CodeBlock>
                  <SectionTitle>Dialect Output</SectionTitle>
                  <CodeBlock label="PostgreSQL">{`SELECT * FROM orders
WHERE (customer_id IN (
  SELECT id FROM customers
  WHERE (region = $1)
));`}</CodeBlock>
                  <CodeBlock label="SQLite / DuckDB / MySQL">{`SELECT * FROM orders
WHERE (customer_id IN (
  SELECT id FROM customers
  WHERE (region = ?)
));`}</CodeBlock>
                </div>
              )}

              {/* ── DDL ── */}
              {activeDocSection === 'ddl' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Table Schema (DDL)</h1>
                  <CodeBlock label="PipeQL">{`table notes [
  id int primary auto,
  title string not null,
  content string not null,
  category string default 'Personal',
  is_pinned int default 0,
  created_at timestamp default current_timestamp
]`}</CodeBlock>
                  <div className="grid grid-cols-2 gap-3">
                    <CodeBlock label="PostgreSQL">{`CREATE TABLE IF NOT EXISTS notes (
  id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  title TEXT NOT NULL, content TEXT NOT NULL,
  category TEXT DEFAULT 'Personal',
  is_pinned INTEGER DEFAULT 0,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);`}</CodeBlock>
                    <CodeBlock label="SQLite">{`CREATE TABLE IF NOT EXISTS notes (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL, content TEXT NOT NULL,
  category TEXT DEFAULT 'Personal',
  is_pinned INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);`}</CodeBlock>
                  </div>
                  <SectionTitle>Type Mapping</SectionTitle>
                  <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                    <table className="w-full text-left text-xs bg-surface-container-lowest">
                      <thead><tr className="bg-surface border-b border-outline-variant/20 text-[9px] font-bold uppercase"><th className="px-3 py-2">PipeQL</th><th className="px-3 py-2">PostgreSQL</th><th className="px-3 py-2">SQLite</th><th className="px-3 py-2">DuckDB</th><th className="px-3 py-2">MySQL</th></tr></thead>
                      <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant">
                        <tr><td className="px-3 py-2 font-mono text-primary">int</td><td className="px-3 py-2">INTEGER</td><td className="px-3 py-2">INTEGER</td><td className="px-3 py-2">INTEGER</td><td className="px-3 py-2">INT</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">string</td><td className="px-3 py-2">TEXT</td><td className="px-3 py-2">TEXT</td><td className="px-3 py-2">VARCHAR</td><td className="px-3 py-2">VARCHAR(255)</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">bool</td><td className="px-3 py-2">BOOLEAN</td><td className="px-3 py-2">INTEGER</td><td className="px-3 py-2">BOOLEAN</td><td className="px-3 py-2">BOOLEAN</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">timestamp</td><td className="px-3 py-2">TIMESTAMP</td><td className="px-3 py-2">DATETIME</td><td className="px-3 py-2">TIMESTAMP</td><td className="px-3 py-2">TIMESTAMP</td></tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              )}

              {/* ── API REFERENCE ── */}
              {activeDocSection === 'api-reference' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">API Reference</h1>
                  <SectionTitle>JavaScript / TypeScript</SectionTitle>
                  <CodeBlock label="@flaxmbot/pipeql">{`import { compile, compileWithCatalog, parse, supportedDialects, version } from '@flaxmbot/pipeql';

// Basic compile
const r = compile("from users | take 5", "postgres");
r.sql;           // "SELECT * FROM users LIMIT 5;"
r.params;        // []
r.statementType; // "select"
r.isMutation;    // false
r.parameterCount; // 0

// Compile with schema validation
const catalog = JSON.stringify({ tables: [{ name: "users", columns: ["id", "name"] }] });
const r2 = compileWithCatalog("from users | take 5", "postgres", catalog);

// Parse to AST
const ast = parse("from users | filter id == 1");

// List dialects
supportedDialects(); // ["postgres", "sqlite", "duckdb", "mysql"]`}</CodeBlock>
                  <SectionTitle>Python</SectionTitle>
                  <CodeBlock label="pipeql_python">{`import pipeql_python as pipeql

# Basic compile
r = pipeql.compile("from users | take 5", "postgres")
r["sql"]  # "SELECT * FROM users LIMIT 5;"
r["parameter_count"]  # 0

# Compile with schema validation
r2 = pipeql.compile_with_catalog("from users", "postgres", '{"tables":[{"name":"users"}]}')

# Parse to AST
ast = pipeql.parse("from users | filter id == 1")

# List dialects
pipeql.supported_dialects()  # ["postgres", "sqlite", "duckdb", "mysql"]`}</CodeBlock>
                  <SectionTitle>C (CFFI)</SectionTitle>
                  <CodeBlock label="libpipeql.h">{`// Basic compile
PipeqlError err = {0};
PipeqlResult* res = pipeql_compile("from users | take 5", "postgres", &err);
if (res) {
    printf("%s\\n", res->sql);            // "SELECT * FROM users LIMIT 5;"
    printf("%d\\n", res->parameter_count); // 0
    pipeql_result_free(res);
} else {
    printf("Error: %s\\n", err.message);
    pipeql_error_clear(&err);
}

// Compile with schema validation
const char* catalog = "{\\\"tables\\\":[{\\\"name\\\":\\\"users\\\"}]}";
PipeqlResult* r2 = pipeql_compile_with_catalog(
    "from users | take 5", "postgres", catalog, &err);

// Parse to AST (returns JSON string)
char* ast = pipeql_parse("from users | filter id == 1", &err);
if (ast) { printf("%s\\n", ast); pipeql_string_free(ast); }

// List supported dialects
char* dialects = pipeql_supported_dialects(&err);
if (dialects) { printf("%s\\n", dialects); pipeql_string_free(dialects); }`}</CodeBlock>
                  <SectionTitle>Go</SectionTitle>
                  <CodeBlock label="go">{`import "github.com/Flaxmbot/PipeQL/go"

// Basic compile
res, err := pipeql.Compile("from users | take 5", "postgres")
fmt.Println(res.SQL)             // "SELECT * FROM users LIMIT 5;"
fmt.Println(res.ParameterCount)  // 0

// Compile with schema validation
catalog := \`{"tables":[{"name":"users","columns":["id","name"]}]}\`
res2, err := pipeql.CompileWithCatalog("from users | take 5", "postgres", catalog)

// Parse to AST
ast, err := pipeql.Parse("from users | filter id == 1")
fmt.Println(string(ast))  // JSON AST

// List dialects
dialects := pipeql.SupportedDialects()
fmt.Println(dialects)  // [postgres sqlite duckdb mysql]`}</CodeBlock>
                  <SectionTitle>CLI</SectionTitle>
                  <CodeBlock label="Terminal">{`# Install
cargo install pipeql-cli

# Compile a query
pipeql compile "from users | take 5" --dialect postgres

# Compile with JSON output
pipeql compile "from users | take 5" --json

# Compile with schema validation
pipeql compile "from users | take 5" --catalog schema.json

# Parse to JSON AST
pipeql parse "from users | filter id == 1"

# List supported dialects
pipeql supported-dialects`}</CodeBlock>
                </div>
              )}

              {/* ── DRIVERS ── */}
              {activeDocSection === 'drivers' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Driver Adapters</h1>
                  <SectionTitle>JavaScript</SectionTitle>
                  <CodeBlock label="@flaxmbot/pipeql/driver">{`import { createPipeqlDriver } from '@flaxmbot/pipeql/driver';
import sqlite3 from 'sqlite3';

const db = createPipeqlDriver(new sqlite3.Database('app.db'), { dialect: 'sqlite' });

// Query (SELECT)
const rows = await db.query('from users | filter role == $role', { role: 'admin' });

// Execute (INSERT/UPDATE/DELETE)
const res = await db.execute('into users | insert [name = $name]', { name: 'Alice' });

// Write + Return
const note = await db.insertAndFetch('into notes | insert $data', { title: 'Hi' });`}</CodeBlock>
                  <SectionTitle>Python</SectionTitle>
                  <CodeBlock label="pipeql_python.driver">{`import sqlite3
from pipeql_python.driver import create_pipeql_driver

db = create_pipeql_driver(sqlite3.connect('app.db'))
rows = db.query("from users | filter role == $role", {"role": "admin"})
note = db.insert_and_fetch("into notes | insert $data", {"title": "Hi"})`}</CodeBlock>
                  <SectionTitle>$data Expansion</SectionTitle>
                  <CodeBlock label="example">{`// Pass partial object — keys become columns
await db.execute('from notes | filter id == $id | update $data', {
  id: req.params.id,
  data: req.body  // only sent fields are updated
});`}</CodeBlock>
                </div>
              )}

              {/* ── FLUENT BUILDER ── */}
              {activeDocSection === 'builder' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Fluent Builder (Optional)</h1>
                  <p className="text-on-surface-variant text-sm leading-relaxed">
                    The PipeQL <strong>string DSL is the primary interface</strong>. For most queries
                    you write the pipeline directly — it is shorter, and just as safe:
                  </p>
                  <CodeBlock label="PipeQL (primary interface)">{`from notes
| filter is_archived == 0
| sort [created_at desc]
| take 10`}</CodeBlock>
                  <p className="text-on-surface-variant text-sm leading-relaxed">
                    When you need to compose queries <strong>programmatically</strong> — conditional
                    or looped pipeline stages, or object-style inserts — every SDK additionally ships
                    an <strong>optional fluent builder</strong> that composes the exact same PipeQL
                    source string a hand-written query would use. A builder query and a literal
                    string are <em>provably identical</em>: no dual parser, no semantic drift.
                  </p>
                  <p className="text-on-surface-variant text-sm leading-relaxed">
                    Object inserts and updates accept key → value objects and auto-generate{' '}
                    <InlineCode>$b0</InlineCode>, <InlineCode>$b1</InlineCode>, ... bind parameters — the{' '}
                    <InlineCode>$data</InlineCode> ergonomics without needing a driver.
                  </p>

                  <SectionTitle>Same query, five SDKs</SectionTitle>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <CodeBlock label="Rust">{`use pipeql_core::builder::{Query, Value};

let q = Query::from("notes")
    .filter("is_archived == 0")
    .sort(["created_at desc"])
    .take(10);

let sql = q.compile("postgres").unwrap().sql;

// Object insert -> auto params $b0, $b1...
let ins = Query::into_("notes").insert([
    ("title", Value::Str("Hi".into())),
    ("flag", Value::Int(1)),
]);
// source: "into notes | insert [title = $b0, flag = $b1]"`}</CodeBlock>
                    <CodeBlock label="JavaScript / TypeScript">{`import { PipeQL } from '@flaxmbot/pipeql/builder';

const q = PipeQL.from('notes')
  .filter('is_archived == 0')
  .sort(['created_at desc'])
  .take(10);

const { sql, params } = await q.compile('postgres');

// Object insert -> auto params $b0, $b1...
const ins = PipeQL.into('notes').insert({ title: 'Hi', flag: 1 });
// source: "into notes | insert [title = $b0, flag = $b1]"
// values: { b0: 'Hi', b1: 1 }`}</CodeBlock>
                    <CodeBlock label="Python">{`from pipeql_python.builder import PipeQL

q = (PipeQL.from_("notes")
     .filter("is_archived == 0")
     .sort(["created_at desc"])
     .take(10))

result = q.compile("postgres")
rows = db.query(q)  # works through any PipeqlDriver

# Object insert -> auto params $b0, $b1...
ins = PipeQL.into_("notes").insert({"title": "Hi", "flag": 1})`}</CodeBlock>
                    <CodeBlock label="Go">{`q := pipeql.From("notes").
    Filter("is_archived == 0").
    Sort([]string{"created_at desc"}).
    Take(10)

res, err := q.Compile("postgres")

// Maps are sorted for deterministic SQL;
// PairsOf keeps the exact column order.
ins := pipeql.Into("notes").Insert(
    pipeql.PairsOf("title", "Hi", "flag", 1))
// source: "into notes | insert [title = $b0, flag = $b1]"`}</CodeBlock>
                    <CodeBlock label="C (libpipeql)" className="md:col-span-2">{`PipeqlError err = {0};
PipeqlQuery* q = pipeql_query_from("notes");
q = pipeql_query_filter(q, "is_archived == 0");
q = pipeql_query_sort(q, "created_at desc");
q = pipeql_query_take(q, 10);

PipeqlResult* res = pipeql_query_compile(q, "postgres", &err);
printf("%s\\n", res->sql);
pipeql_result_free(res);
pipeql_query_free(q);`}</CodeBlock>
                  </div>

                  <SectionTitle>Stage reference</SectionTitle>
                  <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                    <table className="w-full text-left text-xs bg-surface-container-lowest">
                      <thead><tr className="bg-surface border-b border-outline-variant/20 text-[9px] font-bold uppercase tracking-wider"><th className="px-3 py-2">Stage</th><th className="px-3 py-2">Rust / Python</th><th className="px-3 py-2">JS / TS</th><th className="px-3 py-2">Go</th><th className="px-3 py-2">C</th></tr></thead>
                      <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant">
                        <tr><td className="px-3 py-2 font-mono text-primary">from</td><td className="px-3 py-2 font-mono">Query::from / from_</td><td className="px-3 py-2 font-mono">PipeQL.from</td><td className="px-3 py-2 font-mono">From</td><td className="px-3 py-2 font-mono">pipeql_query_from</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">into</td><td className="px-3 py-2 font-mono">Query::into_ / into_</td><td className="px-3 py-2 font-mono">PipeQL.into</td><td className="px-3 py-2 font-mono">Into</td><td className="px-3 py-2 font-mono">pipeql_query_into</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">filter</td><td className="px-3 py-2 font-mono">.filter(expr)</td><td className="px-3 py-2 font-mono">.filter(expr)</td><td className="px-3 py-2 font-mono">.Filter(expr)</td><td className="px-3 py-2 font-mono">pipeql_query_filter</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">select / derive / sort</td><td className="px-3 py-2 font-mono">.select([..]) etc.</td><td className="px-3 py-2 font-mono">.select([..]) etc.</td><td className="px-3 py-2 font-mono">.Select(..) etc.</td><td className="px-3 py-2 font-mono">pipeql_query_select</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">take / skip</td><td className="px-3 py-2 font-mono">.take(n) / .skip(n)</td><td className="px-3 py-2 font-mono">.take(n) / .skip(n)</td><td className="px-3 py-2 font-mono">.Take(n) / .Skip(n)</td><td className="px-3 py-2 font-mono">pipeql_query_take</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">joins</td><td className="px-3 py-2 font-mono">.left_join(t, on)</td><td className="px-3 py-2 font-mono">.leftJoin(t, on)</td><td className="px-3 py-2 font-mono">.LeftJoin(t, on)</td><td className="px-3 py-2 font-mono">pipeql_query_left_join</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">group</td><td className="px-3 py-2 font-mono">.group(cols, aggs)</td><td className="px-3 py-2 font-mono">.group(cols, aggs)</td><td className="px-3 py-2 font-mono">.Group(cols, aggs)</td><td className="px-3 py-2 font-mono">pipeql_query_group</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">union</td><td className="px-3 py-2 font-mono">.union(other)</td><td className="px-3 py-2 font-mono">.union(other)</td><td className="px-3 py-2 font-mono">.Union(other)</td><td className="px-3 py-2 font-mono">pipeql_query_union</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">insert / update / delete</td><td className="px-3 py-2 font-mono">.insert({'{...}'}) etc.</td><td className="px-3 py-2 font-mono">.insert({'{...}'}) etc.</td><td className="px-3 py-2 font-mono">.Insert(map) etc.</td><td className="px-3 py-2 font-mono">pipeql_query_insert</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">upsert chain</td><td className="px-3 py-2 font-mono">.upsert / .conflict / .do_update</td><td className="px-3 py-2 font-mono">.upsert / .conflict / .doUpdate</td><td className="px-3 py-2 font-mono">.Upsert / .Conflict / .DoUpdate</td><td className="px-3 py-2 font-mono">pipeql_query_upsert</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">source text</td><td className="px-3 py-2 font-mono">.source()</td><td className="px-3 py-2 font-mono">.source()</td><td className="px-3 py-2 font-mono">.Source()</td><td className="px-3 py-2 font-mono">pipeql_query_source</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">compile</td><td className="px-3 py-2 font-mono">.compile(dialect)</td><td className="px-3 py-2 font-mono">.compile(dialect)</td><td className="px-3 py-2 font-mono">.Compile(dialect)</td><td className="px-3 py-2 font-mono">pipeql_query_compile</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">bound values</td><td className="px-3 py-2 font-mono">.values</td><td className="px-3 py-2 font-mono">.values</td><td className="px-3 py-2 font-mono">.Values()</td><td className="px-3 py-2 font-mono">—</td></tr>
                      </tbody>
                    </table>
                  </div>

                  <SectionTitle>Drivers accept builders</SectionTitle>
                  <p className="text-sm text-on-surface-variant leading-relaxed">
                    In the JS and Python SDKs, driver methods duck-type builders — pass a{' '}
                    <InlineCode>PipeQL</InlineCode> instance anywhere you would pass a source string,
                    and builder-generated values merge with your params automatically.
                  </p>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                    <CodeBlock label="JavaScript">{`import { PipeQL } from '@flaxmbot/pipeql/builder';

const q = PipeQL.from('notes')
  .filter('category == $cat')
  .take(10);

const rows = await db.query(q, { cat: 'Ideas' });
// builder values + explicit params merge`}</CodeBlock>
                    <CodeBlock label="Python">{`from pipeql_python.builder import PipeQL

q = PipeQL.from_("notes").filter("category == $cat").take(10)

rows = db.query(q, {"cat": "Ideas"})
# builder values + explicit params merge`}</CodeBlock>
                  </div>

                  <Warning type="info">
                    Builder methods take <InlineCode>$name</InlineCode>-style expressions (same as
                    string queries). Object inserts auto-generate <InlineCode>$b0</InlineCode>,
                    <InlineCode>$b1</InlineCode>, ... so a compiled insert is ready to bind without
                    any manual param naming.
                  </Warning>
                </div>
              )}

              {/* ── LSP ── */}
              {activeDocSection === 'lsp' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">LSP & VS Code</h1>
                  <SectionTitle>Language Server</SectionTitle>
                  <CodeBlock label="Terminal">{`cargo build --release -p pipeql-lsp
./target/release/pipeql-lsp`}</CodeBlock>
                  <div className="grid grid-cols-3 gap-3">
                    {[['error','Diagnostics'],['text_format','Completion'],['info','Hover']].map(([i,t]) => (
                      <div key={t} className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl p-3 text-center">
                        <span className={`material-symbols-outlined text-lg mb-1 ${i==='error'?'text-primary':i==='text_format'?'text-tertiary':'text-g-yellow'}`}>{i}</span>
                        <h4 className="text-[11px] font-bold">{t}</h4>
                      </div>
                    ))}
                  </div>
                  <SectionTitle>VS Code Extension</SectionTitle>
                  <ul className="list-disc pl-5 space-y-1 text-xs text-on-surface-variant">
                    <li>Syntax highlighting (TextMate + tree-sitter)</li>
                    <li>11 snippets for all statement types</li>
                    <li>LSP integration (diagnostics, completion, hover)</li>
                    <li>Compile command via Command Palette</li>
                  </ul>
                  <SectionTitle>Settings</SectionTitle>
                  <div className="overflow-x-auto rounded-2xl border border-outline-variant/30">
                    <table className="w-full text-left text-xs bg-surface-container-lowest">
                      <thead><tr className="bg-surface border-b border-outline-variant/20 text-[9px] font-bold uppercase"><th className="px-3 py-2">Setting</th><th className="px-3 py-2">Default</th><th className="px-3 py-2">Description</th></tr></thead>
                      <tbody className="divide-y divide-outline-variant/15 text-on-surface-variant">
                        <tr><td className="px-3 py-2 font-mono text-primary">pipeql.lsp.enabled</td><td className="px-3 py-2">true</td><td className="px-3 py-2">Enable LSP</td></tr>
                        <tr><td className="px-3 py-2 font-mono text-primary">pipeql.defaultDialect</td><td className="px-3 py-2">"postgres"</td><td className="px-3 py-2">Default dialect</td></tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              )}

              {/* ── TREE-SITTER ── */}
              {activeDocSection === 'tree-sitter' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Tree-sitter Grammar</h1>
                  <CodeBlock label="grammar.js">{`module.exports = grammar({
  name: 'pipeql',
  rules: {
    statement: $ => choice($.pipeline, $.insert_stmt, $.delete_stmt, $.table_stmt),
    pipeline: $ => seq($.source, repeat(seq($.pipe, $.step))),
    source: $ => seq('from', $.ident),
    step: $ => choice($.filter_step, $.select_step, $.join_step, ...),
  }
});`}</CodeBlock>
                  <SectionTitle>Bindings</SectionTitle>
                  <div className="flex gap-2">{['Rust','Node.js','C','Go','Python'].map(l => <div key={l} className="bg-surface-container-lowest border border-outline-variant/20 rounded-2xl px-3 py-1.5 text-[10px] font-bold">{l}</div>)}</div>
                </div>
              )}

              {/* ── ARCHITECTURE ── */}
              {activeDocSection === 'architecture' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Architecture & Security</h1>
                  <SectionTitle>Compilation Pipeline</SectionTitle>
                  <CodeBlock label="pipeline">{`1. LEXING     Hand-written lexer, preserves character positions.
               → crates/pipeql-core/src/lexer.rs

2. PARSING    Pratt parser → lossless AST.
               → crates/pipeql-core/src/parser.rs + ast.rs

3. CODEGEN    Walk AST → extract constants → generate dialect SQL.
               → crates/pipeql-core/src/analyzer.rs + codegen.rs`}</CodeBlock>
                  <SectionTitle>Parameter Isolation</SectionTitle>
                  <CodeBlock label="security">{`// Input:  from users | filter name == 'admin' OR 1=1'
// Parser sees: string literal "admin' OR 1=1"
// Output: WHERE (name = $1)  params: ["admin' OR 1=1"]
// The malicious input becomes a safe parameter value.`}</CodeBlock>
                  <SectionTitle>Project Structure</SectionTitle>
                  <CodeBlock label="crates/">{`pipeql-core/     # Core compiler (#![deny(unsafe_code)])
pipeql-cli/      # CLI tool
pipeql-cffi/     # C ABI shared library
pipeql-wasm/     # WebAssembly
pipeql-python/   # PyO3 bindings
pipeql-lsp/      # Language server`}</CodeBlock>
                </div>
              )}

              {/* ── CONTRIBUTING ── */}
              {activeDocSection === 'contributing' && (
                <div className="space-y-5">
                  <h1 className="text-3xl font-bold text-on-surface mb-2">Contributing</h1>
                  <SectionTitle>Prerequisites</SectionTitle>
                  <ul className="list-disc pl-5 space-y-1 text-xs text-on-surface-variant">
                    <li>Rust (stable)</li><li>Node.js 18+</li><li>Python 3.11+</li><li>Go 1.21+</li><li>wasm-pack, maturin</li>
                  </ul>
                  <SectionTitle>Build & Test</SectionTitle>
                  <CodeBlock label="Terminal">{`cargo build --release
cargo test --workspace
cargo clippy
cargo fmt --check
cd js && node test/smoke.mjs
go test ./go/...`}</CodeBlock>
                  <SectionTitle>CLI Commands</SectionTitle>
                  <CodeBlock label="Terminal">{`# Compile
pipeql compile "from users | take 5" --dialect postgres
pipeql compile "from users | take 5" --catalog schema.json --json

# Parse to AST
pipeql parse "from users | filter id == 1"

# List dialects
pipeql supported-dialects`}</CodeBlock>
                  <Warning type="info"><strong>Rules:</strong> No mocks. No unsafe in core. Transpile &lt;0.5ms.</Warning>
                </div>
              )}
            </main>
          </div>
        )}
      </main>

      {/* ─── Footer ─── */}
      <footer className="w-full py-6 px-6 max-w-container-max mx-auto bg-surface border-t border-outline-variant/30 mt-auto">
        <div className="flex flex-col md:flex-row justify-between items-center gap-3">
          <div className="flex items-center gap-4">
            <a className="text-on-surface-variant hover:text-on-surface transition-colors text-xs cursor-pointer" onClick={() => setActiveTab('about')}>About</a>
            <a className="text-on-surface-variant hover:text-on-surface transition-colors text-xs cursor-pointer" onClick={() => setActiveTab('features')}>Features</a>
            <a className="text-on-surface-variant hover:text-on-surface transition-colors text-xs cursor-pointer" onClick={() => goToDoc('intro')}>Docs</a>
          </div>
          <span className="text-on-surface-variant text-xs">&copy; 2026 PipeQL Team. MIT License.</span>
        </div>
      </footer>
    </div>
  );
}
