/**
 * @pipeql/js/driver — zero-boilerplate PipeQL database adapters.
 *
 * Wraps any native Node database connection and auto-handles:
 *  - compilation (PipeQL source -> dialect SQL)
 *  - parameter binding (named PipeQL params -> positional driver args)
 *  - dispatch (SELECT via `.all()`/`.query()`, mutations via `.run()`)
 *  - `$data` object expansion (partial insert/update with zero boilerplate)
 *  - `insertAndFetch` / `updateAndFetch` (single-call write + return)
 *
 * Supported drivers (auto-detected by duck-typing, or forced via `driver`):
 *  `better-sqlite3`, `sqlite3`, `pg`, `postgres.js`, `mysql2`, `duckdb`
 *
 * ```ts
 * import { createPipeqlDriver } from "@pipeql/js/driver";
 * const db = createPipeqlDriver(rawConnection, { dialect: "sqlite" });
 *
 * const rows = await db.query("from notes | filter category == $cat", { cat: "Ideas" });
 * const { lastId, changes } = await db.execute(
 *   "into notes | insert [title = $title]", { title: "New Note" });
 * const note = await db.insertAndFetch("into notes | insert $data", req.body);
 * const updated = await db.updateAndFetch(
 *   "from notes | filter id == $id | update $data", { id, data: req.body });
 * const result = await db.pipeql`from notes | filter title == ${userInput}`;
 * ```
 */

import { compile, type CompileResult, type Dialect } from "./index.js";

export type DriverKind =
  | "better-sqlite3"
  | "sqlite3"
  | "pg"
  | "postgres.js"
  | "mysql2"
  | "duckdb";

export interface DriverOptions {
  /** Target dialect. Inferred from the driver kind when omitted. */
  dialect?: Dialect;
  /** Force a driver kind instead of duck-typed auto-detection. */
  driver?: DriverKind;
}

export type ParamValues = Record<string, unknown>;

/** A PipeQL source: a raw string or a builder object exposing `source()`. */
export type QuerySource = string | QueryBuilderLike;

/** Minimal duck-typed builder shape accepted by driver methods. */
export interface QueryBuilderLike {
  source(): string;
  values?: ParamValues;
}

/** Result shape for mutations: affected-row metadata plus an empty rows array. */
export type RunResult = {
  lastId?: unknown;
  changes?: unknown;
  rows: [];
};

/** Return type of `query()`/tagged template: rows for selects, run metadata for mutations. */
export type QueryReturn<T = unknown> = T[] | RunResult;

/** Return type of `insertAndFetch()`/`updateAndFetch()`: the affected row(s) or run metadata. */
export type FetchReturn<T = unknown> =
  | T
  | T[]
  | { lastId?: unknown; changes?: number; rows: [] };

const DIALECT_BY_DRIVER: Record<DriverKind, Dialect> = {
  "better-sqlite3": "sqlite",
  sqlite3: "sqlite",
  pg: "postgres",
  "postgres.js": "postgres",
  mysql2: "mysql",
  duckdb: "duckdb",
};

/** `$data` object-expansion token (a bare param inside an insert/update). */
const DATA_TOKEN = /\$data\b/;
const IDENT = /^[A-Za-z_][A-Za-z0-9_]*$/;

function isPlainObject(v: unknown): v is Record<string, unknown> {
  return v != null && typeof v === "object" && !Array.isArray(v);
}

function expandData(source: string, data: unknown): { source: string; values: ParamValues } {
  if (!isPlainObject(data)) {
    throw new Error(
      `[pipeql] $data object expansion requires a plain object (got ${data === null ? "null" : typeof data})`,
    );
  }
  const entries = Object.entries(data).filter(([, v]) => v !== undefined);
  if (entries.length === 0) {
    throw new Error("[pipeql] $data object expansion requires at least one property");
  }
  const parts = source.split(DATA_TOKEN);
  const chunks: string[] = [parts[0]];
  const values: ParamValues = {};
  let n = 0;
  for (let i = 1; i < parts.length; i++) {
    const prev = chunks[chunks.length - 1];
    const last = prev.trimEnd().slice(-1);
    const inBrackets = last === "[" || last === ",";
    const assignments = entries
      .map(([key, v]) => {
        if (!IDENT.test(key)) {
          throw new Error(
            `[pipeql] cannot expand $data: column '${key}' is not a valid identifier`,
          );
        }
        const pname = `data${n++}`;
        values[pname] = v;
        return `${key} = $${pname}`;
      })
      .join(", ");
    chunks.push(inBrackets ? assignments : `[${assignments}]`);
    chunks.push(parts[i]);
  }
  return { source: chunks.join(""), values };
}

function withReturning(sql: string): string {
  if (/\bRETURNING\b/i.test(sql)) return sql;
  return sql.trimEnd().replace(/;\s*$/, "") + " RETURNING *;";
}

export class PipeqlDriver {
  /** Detected (or forced) driver kind. */
  readonly driver: DriverKind;
  /** Target compile dialect. */
  readonly dialect: Dialect;

  private readonly conn: unknown;
  private readonly cache = new Map<string, CompileResult>();
  private readonly mysqlPromise: unknown;

  constructor(conn: unknown, options: DriverOptions = {}) {
    if (conn == null) {
      throw new Error("[pipeql] createPipeqlDriver requires a database connection.");
    }
    this.driver = options.driver ?? detectDriver(conn);
    this.dialect = options.dialect ?? DIALECT_BY_DRIVER[this.driver];
    this.conn = conn;
    this.mysqlPromise =
      this.driver === "mysql2" && typeof (conn as any).promise === "function"
        ? (conn as any).promise()
        : null;
  }

  /** Resolve a builder object (duck-typed) or a raw source string. */
  private sourceOf(source: QuerySource): string {
    if (typeof source === "string") return source;
    return source.source();
  }

  /**
   * Resolve builders, then `$data` object expansion. When the source
   * references `$data`, the data object's keys are expanded into explicit
   * column assignments and its values become bound params.
   * `wholeObjectAsData` treats the entire params object as the data object
   * (used by `insertAndFetch`).
   */
  private prepare(
    source: QuerySource,
    params?: ParamValues,
    wholeObjectAsData = false,
  ): { source: string; params: ParamValues } {
    const builderValues =
      typeof source === "object" && source.values ? source.values : undefined;
    const src = this.sourceOf(source);
    const merged: ParamValues = { ...(params ?? {}), ...(builderValues ?? {}) };
    if (!DATA_TOKEN.test(src)) return { source: src, params: merged };
    const data = wholeObjectAsData ? (merged.data ?? merged) : merged.data;
    const { source: expanded, values } = expandData(src, data);
    return { source: expanded, params: { ...merged, ...values } };
  }

  /** Compile (cached per source) to the driver dialect. */
  private async compiled(source: string): Promise<CompileResult> {
    const key = `${this.dialect}\u0000${source}`;
    let hit = this.cache.get(key);
    if (!hit) {
      hit = await compile(source, this.dialect);
      this.cache.set(key, hit);
    }
    return hit;
  }

  /**
   * Map named PipeQL params to positional driver args.
   *
   * Literal-derived params (a concrete type in the analysis param map, or a
   * select-mode string literal with no param-map entry) bind as themselves.
   * A user `$param` (type `Any`) missing from the values object is a bug —
   * fail loudly instead of silently binding its name and returning wrong data.
   */
  private bind(compiled: CompileResult, params?: ParamValues): unknown[] {
    const types = new Map(
      (compiled.analysis?.param_map ?? []).map((p) => [p.name, p.ty]),
    );
    const has = (params: ParamValues, name: string) =>
      Object.prototype.hasOwnProperty.call(params, name);
    return compiled.params.map((name) => {
      // hasOwnProperty, not `in` — a plain object's prototype chain includes
      // names like "constructor"/"toString" that must never bind by accident.
      if (params != null && has(params, name)) return params[name];
      if (types.get(name) === "Any") {
        throw new Error(
          `[pipeql] missing value for parameter '${name}' — pass it in the params object (e.g. { ${name}: ... })`,
        );
      }
      // Literal-derived param: the value is the name itself.
      return name;
    });
  }

  /** Run a row-returning statement (SELECT or `... RETURNING *`) and fetch rows. */
  private async runQuery(sql: string, args: unknown[]): Promise<unknown[]> {
    switch (this.driver) {
      case "better-sqlite3": {
        const db = this.conn as any;
        return db.prepare(sql).all(...args) as unknown[];
      }
      case "sqlite3":
      case "duckdb": {
        const db = this.conn as any;
        return await new Promise<unknown[]>((resolve, reject) => {
          db.all(sql, args, (err: Error | null, rows?: unknown[]) =>
            err ? reject(err) : resolve(rows ?? []),
          );
        });
      }
      case "pg": {
        const res = (await (this.conn as any).query(sql, args)) as { rows?: unknown[] };
        return res.rows ?? [];
      }
      case "postgres.js": {
        const res = await (this.conn as any).unsafe(sql, args);
        return Array.isArray(res) ? res : [res];
      }
      case "mysql2":
        throw new Error(
          "[pipeql] RETURNING (insertAndFetch/updateAndFetch) is not supported on mysql2",
        );
    }
  }

  private async runRaw(
    compiled: CompileResult,
    args: unknown[],
  ): Promise<{ rows: unknown[]; lastId?: unknown; changes?: unknown }> {
    // select AND union return rows; mutations and DDL go through .run().
    // Keying off `statementType === "select"` dropped rows from `union`
    // queries, which are read-only but not "select" typed.
    const isQuery =
      compiled.statementType === "select" || compiled.statementType === "union";
    switch (this.driver) {
      case "better-sqlite3": {
        const db = this.conn as any;
        const stmt = db.prepare(compiled.sql);
        if (isQuery) return { rows: stmt.all(...args) as unknown[] };
        const r = stmt.run(...args) as { changes: number; lastInsertRowid: unknown };
        return { rows: [], lastId: r.lastInsertRowid, changes: r.changes };
      }
      case "sqlite3": {
        const db = this.conn as any;
        if (isQuery) return { rows: await this.runQuery(compiled.sql, args) };
        const runRes = await new Promise<{ lastId: unknown; changes: unknown }>(
          (resolve, reject) => {
            db.run(compiled.sql, args, function (this: any, err: Error | null) {
              if (err) reject(err);
              else resolve({ lastId: this?.lastID, changes: this?.changes });
            });
          },
        );
        return { rows: [], lastId: runRes.lastId, changes: runRes.changes };
      }
      case "duckdb": {
        const db = this.conn as any;
        if (isQuery) return { rows: await this.runQuery(compiled.sql, args) };
        await new Promise<void>((resolve, reject) => {
          db.run(compiled.sql, args, (err: Error | null) =>
            err ? reject(err) : resolve(),
          );
        });
        return { rows: [] };
      }
      case "pg": {
        const db = this.conn as any;
        const res = (await db.query(compiled.sql, args)) as {
          rows?: unknown[];
          rowCount?: number;
        };
        return { rows: res.rows ?? [], changes: res.rowCount };
      }
      case "postgres.js": {
        const db = this.conn as any;
        const res = (await db.unsafe(compiled.sql, args)) as unknown[] & {
          count?: number;
        };
        const rows = Array.isArray(res) ? res : [res];
        return { rows, changes: res?.count };
      }
      case "mysql2": {
        const db = (this.mysqlPromise ?? this.conn) as any;
        const [result] = (await db.query(compiled.sql, args)) as [
          unknown[] | { insertId?: unknown; affectedRows?: number },
        ];
        if (isQuery) return { rows: Array.isArray(result) ? result : [] };
        const header = Array.isArray(result) ? result[0] : result;
        return {
          rows: [],
          lastId: (header as any)?.insertId,
          changes: (header as any)?.affectedRows,
        };
      }
    }
  }

  /**
   * Compile source and bind params. Returns the compiler result plus the
   * positional `args` array ready for the native driver.
   */
  async compile(
    source: QuerySource,
    params?: ParamValues,
  ): Promise<CompileResult & { args: unknown[] }> {
    const { source: src, params: p } = this.prepare(source, params);
    const compiled = await this.compiled(src);
    return { ...compiled, args: this.bind(compiled, p) };
  }

  /**
   * Run any statement, auto-dispatching `.all()` vs `.run()`. Returns rows for
   * selects; mutations return `{ lastId, changes, rows: [] }`.
   */
  async query<T = unknown>(
    source: QuerySource,
    params?: ParamValues,
  ): Promise<QueryReturn<T>> {
    const { source: src, params: p } = this.prepare(source, params);
    const compiled = await this.compiled(src);
    const raw = await this.runRaw(compiled, this.bind(compiled, p));
    // select AND union are read-only — both must return rows.
    if (compiled.statementType === "select" || compiled.statementType === "union") {
      return raw.rows as T[];
    }
    return { lastId: raw.lastId, changes: raw.changes, rows: [] };
  }

  /**
   * Run a statement and return its execution result: mutations return
   * `{ lastId, changes, rows: [] }`, selects return `{ rows }`.
   */
  async execute(
    source: QuerySource,
    params?: ParamValues,
  ): Promise<RunResult | { rows: unknown[] }> {
    const { source: src, params: p } = this.prepare(source, params);
    const compiled = await this.compiled(src);
    const raw = await this.runRaw(compiled, this.bind(compiled, p));
    if (compiled.statementType === "select" || compiled.statementType === "union") {
      return { rows: raw.rows };
    }
    return { lastId: raw.lastId, changes: raw.changes, rows: [] };
  }

  /**
   * Single-call insert + return. Inserts the record and returns the created
   * row(s) using `RETURNING *` (sqlite, postgres, duckdb). On mysql2 — which
   * lacks `RETURNING` — falls back to run metadata `{ lastId, changes, rows: [] }`.
   *
   * With `$data`, the entire params object is treated as the data object:
   * ```ts
   * const note = await db.insertAndFetch("into notes | insert $data", req.body);
   * ```
   */
  async insertAndFetch<T = Record<string, unknown>>(
    source: QuerySource,
    params?: ParamValues,
  ): Promise<FetchReturn<T>> {
    const { source: src, params: p } = this.prepare(source, params, true);
    const compiled = await this.compiled(src);
    const args = this.bind(compiled, p);
    const rows = await this.runQuery(withReturning(compiled.sql), args);
    if (rows.length === 0) {
      return { lastId: undefined, changes: 0, rows: [] };
    }
    return rows.length === 1 ? (rows[0] as T) : (rows as T[]);
  }

  /**
   * Single-call update + return. Updates matching rows and returns the updated
   * row(s) via `RETURNING *` (sqlite, postgres, duckdb). On mysql2 falls back
   * to run metadata `{ lastId, changes, rows: [] }`.
   *
   * ```ts
   * const note = await db.updateAndFetch(
   *   "from notes | filter id == $id | update $data", { id, data: req.body });
   * ```
   */
  async updateAndFetch<T = Record<string, unknown>>(
    source: QuerySource,
    params?: ParamValues,
  ): Promise<FetchReturn<T>> {
    const { source: src, params: p } = this.prepare(source, params);
    const compiled = await this.compiled(src);
    const args = this.bind(compiled, p);
    const rows = await this.runQuery(withReturning(compiled.sql), args);
    if (rows.length === 0) {
      return { lastId: undefined, changes: 0, rows: [] };
    }
    return rows.length === 1 ? (rows[0] as T) : (rows as T[]);
  }

  /**
   * Tagged template: interpolated values become named bind params (never
   * inlined SQL). Dispatches like {@link query}.
   *
   * ```ts
   * const rows = await db.pipeql`from notes | filter title == ${userInput}`;
   * ```
   */
  pipeql<T = unknown>(
    strings: TemplateStringsArray,
    ...values: unknown[]
  ): Promise<QueryReturn<T>> {
    let source = "";
    strings.forEach((part, i) => {
      source += part;
      if (i < values.length) source += `$p${i}`;
    });
    const params: ParamValues = {};
    values.forEach((v, i) => {
      params[`p${i}`] = v;
    });
    return this.query<T>(source, params);
  }

  /** Best-effort close of the underlying connection when it exposes close/end. */
  async close(): Promise<void> {
    const db = this.conn as any;
    const closer = db?.close ?? db?.end;
    if (typeof closer === "function") {
      const res = closer.call(db);
      if (res && typeof res.then === "function") await res;
    }
  }
}

function detectDriver(conn: unknown): DriverKind {
  const c = conn as { [k: string]: unknown } | undefined;
  const has = (k: string) => typeof c?.[k] === "function";
  if (has("unsafe")) return "postgres.js";
  if (has("prepare") && !has("all")) return "better-sqlite3";
  if (has("all") && has("run")) return typeof c?.on === "function" ? "sqlite3" : "duckdb";
  if (has("query")) {
    if (has("promise") || c?.config != null) return "mysql2";
    if (has("connect") || c?.connectionParameters != null || c?.options != null) {
      return "pg";
    }
    return "mysql2";
  }
  throw new Error(
    "[pipeql] Unsupported database driver: no recognizable query API. Pass { driver } to force a driver kind.",
  );
}

/** Wrap any native database connection with automatic PipeQL compilation and dispatch. */
export function createPipeqlDriver(
  conn: unknown,
  options?: DriverOptions,
): PipeqlDriver {
  return new PipeqlDriver(conn, options);
}
