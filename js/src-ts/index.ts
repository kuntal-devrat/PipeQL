/**
 * @pipeql/js — TypeScript SDK for PipeQL.
 *
 * The SDK wraps the WASM core and exposes:
 *  - `compile(source, dialect?)`       — compile to SQL + parameter map
 *  - `compileWithCatalog(...)`         — compile with schema validation
 *  - `pipeql` tagged template          — ergonomic, injection-safe interpolation
 *  - `parse(source)`                   — lossless AST for tooling
 *  - `supportedDialects()`, `version()`
 */

import init, {
  compile as wasmCompile,
  compileWithCatalog as wasmCompileWithCatalog,
  compileWithSchema as wasmCompileWithSchema,
  catalogFromSchema as wasmCatalogFromSchema,
  parseAst,
  supportedDialects as wasmSupportedDialects,
  version as wasmVersion,
  type Compiled,
} from "../dist/pipeql_wasm.js";

export type Dialect = "postgres" | "sqlite" | "duckdb" | "mysql";

/** The kind of statement a compiled query represents. */
export type StatementType =
  | "select"
  | "insert"
  | "update"
  | "delete"
  | "create_table"
  | "upsert"
  | "union";

export interface ParamMeta {
  name: string;
  ty: string;
  occurrences: number[];
}

export interface Analysis {
  param_map: ParamMeta[];
  validated_columns: boolean;
}

export interface CompileResult {
  /** Target-dialect SQL with positional placeholders (`$1`, `?`, ...). */
  sql: string;
  /** Ordered parameter names (bind values in this order). */
  params: string[];
  /** Statement kind, so you can dispatch `.all()` vs `.run()` without parsing SQL. */
  statementType: StatementType;
  /** True for mutations (insert/update/delete). */
  isMutation: boolean;
  /** Full semantic analysis (param map, types, occurrences). */
  analysis: Analysis;
  /** Number of distinct parameters. */
  parameterCount: number;
}

export interface SchemaColumn {
  name: string;
  /** Analyzer catalog type. */
  ty: "Integer" | "Float" | "String" | "Bool" | "Null" | "Timestamp" | "Any";
}

export interface SchemaTable {
  name: string;
  columns: SchemaColumn[];
}

export type Catalog = Record<string, SchemaTable>;

let initialized = false;

/**
 * Idempotently initialize the WASM module.
 *
 * In the browser the wasm is fetched relative to the module URL. In Node the
 * bytes are read from disk. Safe to call multiple times.
 */
export async function initWasm(): Promise<void> {
  if (initialized) return;
  const isNode =
    typeof process !== "undefined" && process.versions?.node != null;
  if (isNode) {
    const { readFileSync } = await import("node:fs");
    const { dirname, join } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const here = dirname(fileURLToPath(import.meta.url));
    const bytes = readFileSync(join(here, "../dist/pipeql_wasm_bg.wasm"));
    await init({ module_or_path: bytes });
  } else {
    await init();
  }
  initialized = true;
}

function toResult(compiled: Compiled): CompileResult {
  const params = compiled.params as unknown as string[];
  // `analysis` is the most expensive payload to cross the WASM boundary and is
  // unused by most callers; expose it as a memoized getter so the hot sql/params
  // path never pays for it. The Rust side is lazy too, so `compiled.analysis`
  // only serializes on first read.
  let analysis: Analysis | undefined;
  const result: CompileResult = {
    sql: compiled.sql,
    params,
    statementType: compiled.statement_type as StatementType,
    isMutation: compiled.is_mutation as boolean,
    parameterCount: params.length,
    get analysis(): Analysis {
      if (analysis === undefined) analysis = compiled.analysis as Analysis;
      return analysis;
    },
  };
  return result;
}

/**
 * Compile a PipeQL source string for a target dialect.
 *
 * ```ts
 * const { sql, params } = await compile(
 *   "from users | filter age >= $min | select [id, name]",
 *   "postgres",
 * );
 * // sql:    "SELECT id, name FROM users\nWHERE (age >= $1);"
 * // params: ["min"]
 * ```
 */
export async function compile(
  source: string,
  dialect: Dialect = "postgres",
): Promise<CompileResult> {
  await initWasm();
  return toResult(wasmCompile(source, dialect));
}

/**
 * Compile with a schema catalog for column validation. Unknown columns raise.
 */
export async function compileWithCatalog(
  source: string,
  catalog: Catalog,
  dialect: Dialect = "postgres",
): Promise<CompileResult> {
  await initWasm();
  const catalogJson = JSON.stringify({ tables: catalog });
  return toResult(wasmCompileWithCatalog(source, dialect, catalogJson));
}

/**
 * Parse a PipeQL source into a JSON-serializable, lossless AST (spans and
 * comments preserved). Useful for editors and tooling.
 */
export async function parse(source: string): Promise<unknown> {
  await initWasm();
  return parseAst(source);
}

/** PipeQL statements are newline-oriented: a statement starts on a line whose
 * first keyword introduces one (`table <name>`, `from <table>`, `into <table>`). */
const STATEMENT_STARTERS = ["table ", "from ", "into "];

function splitStatements(source: string): string[] {
  const statements: string[] = [];
  let current: string[] = [];
  for (const line of source.split(/\r?\n/)) {
    const stripped = line.trim();
    if (stripped && current.length > 0 && STATEMENT_STARTERS.some((s) => stripped.startsWith(s))) {
      statements.push(current.join("\n"));
      current = [line];
    } else {
      current.push(line);
    }
  }
  if (current.length > 0 && current.some((l) => l.trim().length > 0)) {
    statements.push(current.join("\n"));
  }
  return statements;
}

// Memoized catalog derivation: identical schema strings parse once, not per
// call. `catalogCache` holds completed derivations (never mutations), while
// `catalogInFlight` dedupes concurrent calls for the same schema.
const catalogCache = new Map<string, Catalog>();
const catalogInFlight = new Map<string, Promise<Catalog>>();

/** Defensive copy so callers can't mutate the shared cached catalog. */
function cloneCatalog(catalog: Catalog): Catalog {
  const out: Catalog = {};
  for (const [name, table] of Object.entries(catalog)) {
    out[name] = { name: table.name, columns: table.columns.map((c) => ({ name: c.name, ty: c.ty })) };
  }
  return out;
}

async function buildCatalog(schema: string): Promise<Catalog> {
  await initWasm();
  if (typeof wasmCatalogFromSchema === "function") {
    return wasmCatalogFromSchema(schema) as unknown as Catalog;
  }
  const catalog: Catalog = {};
  for (const statement of splitStatements(schema)) {
    if (!statement.trimStart().startsWith("table ")) continue;
    let ast: {
      name: { name: string };
      columns: { name: { name: string }; ty: SchemaColumn["ty"] }[];
    };
    try {
      ast = parseAst(statement) as typeof ast;
    } catch (err) {
      throw new Error(`failed to parse table statement: ${(err as Error).message}`);
    }
    const name = ast.name.name;
    if (name in catalog) {
      throw new Error(`duplicate table '${name}' in schema`);
    }
    catalog[name] = {
      name,
      columns: ast.columns.map((c) => ({
        name: c.name.name,
        ty: c.ty,
      })),
    };
  }
  if (Object.keys(catalog).length === 0) {
    throw new Error("catalogFromSchema requires at least one `table` statement");
  }
  return catalog;
}

/**
 * Derive an analyzer catalog from one or more PipeQL `table` statements.
 *
 * The same string that defines your DDL becomes the schema the analyzer
 * validates against — no hand-written catalog, nothing to keep in sync.
 *
 * The derivation is memoized per schema string (concurrent calls for the
 * same schema share one parse; failures are not cached), so `compileWithSchema`
 * in a hot path pays the DDL parse cost only once.
 *
 * ```ts
 * const catalog = await catalogFromSchema(`
 *   table users [id integer primary auto, name string not null]
 *   table posts [id integer primary auto, user_id integer, title string]
 * `);
 * // { users: { name: "users", columns: [{ name: "id", ty: "Integer" }, ...] }, ... }
 * ```
 */
export async function catalogFromSchema(schema: string): Promise<Catalog> {
  const cached = catalogCache.get(schema);
  if (cached) return cloneCatalog(cached);

  let inFlight = catalogInFlight.get(schema);
  if (!inFlight) {
    inFlight = buildCatalog(schema);
    catalogInFlight.set(schema, inFlight);
  }
  try {
    const catalog = await inFlight;
    catalogCache.set(schema, catalog);
    return cloneCatalog(catalog);
  } finally {
    catalogInFlight.delete(schema);
  }
}

/**
 * Compile with analyzer validation, deriving the catalog from `schema` DDL.
 *
 * One call instead of three: pass your `table` statements and get column/type
 * checking against them for free.
 *
 * ```ts
 * const { sql } = await compileWithSchema(
 *   "from users | filter nme == $x",
 *   "table users [id integer primary auto, name string]",
 * );
 * // throws: Unknown column 'nme'
 * ```
 */
export async function compileWithSchema(
  source: string,
  schema: string,
  dialect: Dialect = "postgres",
): Promise<CompileResult> {
  await initWasm();
  if (typeof wasmCompileWithSchema === "function") {
    return toResult(wasmCompileWithSchema(source, dialect, schema));
  }
  const catalog = await catalogFromSchema(schema);
  return compileWithCatalog(source, catalog, dialect);
}

/** List of supported target dialects. */
export async function supportedDialects(): Promise<Dialect[]> {
  await initWasm();
  return wasmSupportedDialects() as unknown as Dialect[];
}

/** PipeQL version. */
export async function version(): Promise<string> {
  await initWasm();
  return wasmVersion();
}

/**
 * Tagged template for ergonomic, injection-safe queries. Interpolated values
 * become named bind parameters (`p0`, `p1`, ...), never inlined SQL.
 *
 * ```ts
 * const q = pipeql`from users | filter age >= ${18} and plan == ${"pro"} | select [id]`;
 * const { sql, params, values } = await q.compile("postgres");
 * // sql:    "SELECT id FROM users\nWHERE ((age >= $1) AND (plan = $2));"
 * // params: ["p0", "p1"]
 * // values: [18, "pro"]
 * ```
 */
export function pipeql(
  strings: TemplateStringsArray,
  ...values: unknown[]
): PipeqlTemplate {
  let source = "";
  strings.forEach((part, i) => {
    source += part;
    if (i < values.length) source += `$p${i}`;
  });
  return new PipeqlTemplate(source, values);
}

export class PipeqlTemplate {
  /** Raw interpolated values in placeholder order. */
  readonly values: unknown[];

  /** Build the PipeQL source (each interpolation replaced by `$pN`). */
  readonly source: string;

  constructor(source: string, values: unknown[]) {
    this.source = source;
    this.values = values;
  }

  /** Compile to a dialect; `values` carry the bound parameters. */
  async compile(dialect: Dialect = "postgres"): Promise<CompileResult & { values: unknown[] }> {
    const result = await compile(this.source, dialect);
    return { ...result, values: this.values };
  }

  /** Alias of {@link compile}. */
  for(dialect: Dialect = "postgres"): ReturnType<PipeqlTemplate["compile"]> {
    return this.compile(dialect);
  }
}
