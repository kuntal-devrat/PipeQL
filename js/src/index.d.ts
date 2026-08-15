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
export type Dialect = "postgres" | "sqlite" | "duckdb" | "mysql";
/** The kind of statement a compiled query represents. */
export type StatementType = "select" | "insert" | "update" | "delete" | "create_table" | "upsert" | "union";
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
/**
 * Idempotently initialize the WASM module.
 *
 * In the browser the wasm is fetched relative to the module URL. In Node the
 * bytes are read from disk. Safe to call multiple times.
 */
export declare function initWasm(): Promise<void>;
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
export declare function compile(source: string, dialect?: Dialect): Promise<CompileResult>;
/**
 * Compile with a schema catalog for column validation. Unknown columns raise.
 */
export declare function compileWithCatalog(source: string, catalog: Catalog, dialect?: Dialect): Promise<CompileResult>;
/**
 * Parse a PipeQL source into a JSON-serializable, lossless AST (spans and
 * comments preserved). Useful for editors and tooling.
 */
export declare function parse(source: string): Promise<unknown>;
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
export declare function catalogFromSchema(schema: string): Promise<Catalog>;
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
export declare function compileWithSchema(source: string, schema: string, dialect?: Dialect): Promise<CompileResult>;
/** List of supported target dialects. */
export declare function supportedDialects(): Promise<Dialect[]>;
/** PipeQL version. */
export declare function version(): Promise<string>;
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
export declare function pipeql(strings: TemplateStringsArray, ...values: unknown[]): PipeqlTemplate;
export declare class PipeqlTemplate {
    /** Raw interpolated values in placeholder order. */
    readonly values: unknown[];
    /** Build the PipeQL source (each interpolation replaced by `$pN`). */
    readonly source: string;
    constructor(source: string, values: unknown[]);
    /** Compile to a dialect; `values` carry the bound parameters. */
    compile(dialect?: Dialect): Promise<CompileResult & {
        values: unknown[];
    }>;
    /** Alias of {@link compile}. */
    for(dialect?: Dialect): ReturnType<PipeqlTemplate["compile"]>;
}
