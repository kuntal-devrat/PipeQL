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
import { compile } from "./index.js";
const DIALECT_BY_DRIVER = {
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
function isPlainObject(v) {
    return v != null && typeof v === "object" && !Array.isArray(v);
}
function expandData(source, data) {
    if (!isPlainObject(data)) {
        throw new Error(`[pipeql] $data object expansion requires a plain object (got ${data === null ? "null" : typeof data})`);
    }
    const entries = Object.entries(data).filter(([, v]) => v !== undefined);
    if (entries.length === 0) {
        throw new Error("[pipeql] $data object expansion requires at least one property");
    }
    const parts = source.split(DATA_TOKEN);
    const chunks = [parts[0]];
    const values = {};
    let n = 0;
    for (let i = 1; i < parts.length; i++) {
        const prev = chunks[chunks.length - 1];
        const last = prev.trimEnd().slice(-1);
        const inBrackets = last === "[" || last === ",";
        const assignments = entries
            .map(([key, v]) => {
            if (!IDENT.test(key)) {
                throw new Error(`[pipeql] cannot expand $data: column '${key}' is not a valid identifier`);
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
function withReturning(sql) {
    if (/\bRETURNING\b/i.test(sql))
        return sql;
    return sql.trimEnd().replace(/;\s*$/, "") + " RETURNING *;";
}
export class PipeqlDriver {
    constructor(conn, options = {}) {
        this.cache = new Map();
        if (conn == null) {
            throw new Error("[pipeql] createPipeqlDriver requires a database connection.");
        }
        this.driver = options.driver ?? detectDriver(conn);
        this.dialect = options.dialect ?? DIALECT_BY_DRIVER[this.driver];
        this.conn = conn;
        this.mysqlPromise =
            this.driver === "mysql2" && typeof conn.promise === "function"
                ? conn.promise()
                : null;
    }
    /** Resolve a builder object (duck-typed) or a raw source string. */
    sourceOf(source) {
        if (typeof source === "string")
            return source;
        return source.source();
    }
    /**
     * Resolve builders, then `$data` object expansion. When the source
     * references `$data`, the data object's keys are expanded into explicit
     * column assignments and its values become bound params.
     * `wholeObjectAsData` treats the entire params object as the data object
     * (used by `insertAndFetch`).
     */
    prepare(source, params, wholeObjectAsData = false) {
        const builderValues = typeof source === "object" && source.values ? source.values : undefined;
        const src = this.sourceOf(source);
        const merged = { ...(params ?? {}), ...(builderValues ?? {}) };
        if (!DATA_TOKEN.test(src))
            return { source: src, params: merged };
        const data = wholeObjectAsData ? (merged.data ?? merged) : merged.data;
        const { source: expanded, values } = expandData(src, data);
        return { source: expanded, params: { ...merged, ...values } };
    }
    /** Compile (cached per source) to the driver dialect. */
    async compiled(source) {
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
    bind(compiled, params) {
        const types = new Map((compiled.analysis?.param_map ?? []).map((p) => [p.name, p.ty]));
        const has = (params, name) => Object.prototype.hasOwnProperty.call(params, name);
        return compiled.params.map((name) => {
            // hasOwnProperty, not `in` — a plain object's prototype chain includes
            // names like "constructor"/"toString" that must never bind by accident.
            if (params != null && has(params, name))
                return params[name];
            if (types.get(name) === "Any") {
                throw new Error(`[pipeql] missing value for parameter '${name}' — pass it in the params object (e.g. { ${name}: ... })`);
            }
            // Literal-derived param: the value is the name itself.
            return name;
        });
    }
    /** Run a row-returning statement (SELECT or `... RETURNING *`) and fetch rows. */
    async runQuery(sql, args) {
        switch (this.driver) {
            case "better-sqlite3": {
                const db = this.conn;
                return db.prepare(sql).all(...args);
            }
            case "sqlite3":
            case "duckdb": {
                const db = this.conn;
                return await new Promise((resolve, reject) => {
                    db.all(sql, args, (err, rows) => err ? reject(err) : resolve(rows ?? []));
                });
            }
            case "pg": {
                const res = (await this.conn.query(sql, args));
                return res.rows ?? [];
            }
            case "postgres.js": {
                const res = await this.conn.unsafe(sql, args);
                return Array.isArray(res) ? res : [res];
            }
            case "mysql2":
                throw new Error("[pipeql] RETURNING (insertAndFetch/updateAndFetch) is not supported on mysql2");
        }
    }
    async runRaw(compiled, args) {
        // select AND union return rows; mutations and DDL go through .run().
        // Keying off `statementType === "select"` dropped rows from `union`
        // queries, which are read-only but not "select" typed.
        const isQuery = compiled.statementType === "select" || compiled.statementType === "union";
        switch (this.driver) {
            case "better-sqlite3": {
                const db = this.conn;
                const stmt = db.prepare(compiled.sql);
                if (isQuery)
                    return { rows: stmt.all(...args) };
                const r = stmt.run(...args);
                return { rows: [], lastId: r.lastInsertRowid, changes: r.changes };
            }
            case "sqlite3": {
                const db = this.conn;
                if (isQuery)
                    return { rows: await this.runQuery(compiled.sql, args) };
                const runRes = await new Promise((resolve, reject) => {
                    db.run(compiled.sql, args, function (err) {
                        if (err)
                            reject(err);
                        else
                            resolve({ lastId: this?.lastID, changes: this?.changes });
                    });
                });
                return { rows: [], lastId: runRes.lastId, changes: runRes.changes };
            }
            case "duckdb": {
                const db = this.conn;
                if (isQuery)
                    return { rows: await this.runQuery(compiled.sql, args) };
                await new Promise((resolve, reject) => {
                    db.run(compiled.sql, args, (err) => err ? reject(err) : resolve());
                });
                return { rows: [] };
            }
            case "pg": {
                const db = this.conn;
                const res = (await db.query(compiled.sql, args));
                return { rows: res.rows ?? [], changes: res.rowCount };
            }
            case "postgres.js": {
                const db = this.conn;
                const res = (await db.unsafe(compiled.sql, args));
                const rows = Array.isArray(res) ? res : [res];
                return { rows, changes: res?.count };
            }
            case "mysql2": {
                const db = (this.mysqlPromise ?? this.conn);
                const [result] = (await db.query(compiled.sql, args));
                if (isQuery)
                    return { rows: Array.isArray(result) ? result : [] };
                const header = Array.isArray(result) ? result[0] : result;
                return {
                    rows: [],
                    lastId: header?.insertId,
                    changes: header?.affectedRows,
                };
            }
        }
    }
    /**
     * Compile source and bind params. Returns the compiler result plus the
     * positional `args` array ready for the native driver.
     */
    async compile(source, params) {
        const { source: src, params: p } = this.prepare(source, params);
        const compiled = await this.compiled(src);
        return { ...compiled, args: this.bind(compiled, p) };
    }
    /**
     * Run any statement, auto-dispatching `.all()` vs `.run()`. Returns rows for
     * selects; mutations return `{ lastId, changes, rows: [] }`.
     */
    async query(source, params) {
        const { source: src, params: p } = this.prepare(source, params);
        const compiled = await this.compiled(src);
        const raw = await this.runRaw(compiled, this.bind(compiled, p));
        // select AND union are read-only — both must return rows.
        if (compiled.statementType === "select" || compiled.statementType === "union") {
            return raw.rows;
        }
        return { lastId: raw.lastId, changes: raw.changes, rows: [] };
    }
    /**
     * Run a statement and return its execution result: mutations return
     * `{ lastId, changes, rows: [] }`, selects return `{ rows }`.
     */
    async execute(source, params) {
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
    async insertAndFetch(source, params) {
        const { source: src, params: p } = this.prepare(source, params, true);
        const compiled = await this.compiled(src);
        const args = this.bind(compiled, p);
        const rows = await this.runQuery(withReturning(compiled.sql), args);
        if (rows.length === 0) {
            return { lastId: undefined, changes: 0, rows: [] };
        }
        return rows.length === 1 ? rows[0] : rows;
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
    async updateAndFetch(source, params) {
        const { source: src, params: p } = this.prepare(source, params);
        const compiled = await this.compiled(src);
        const args = this.bind(compiled, p);
        const rows = await this.runQuery(withReturning(compiled.sql), args);
        if (rows.length === 0) {
            return { lastId: undefined, changes: 0, rows: [] };
        }
        return rows.length === 1 ? rows[0] : rows;
    }
    /**
     * Tagged template: interpolated values become named bind params (never
     * inlined SQL). Dispatches like {@link query}.
     *
     * ```ts
     * const rows = await db.pipeql`from notes | filter title == ${userInput}`;
     * ```
     */
    pipeql(strings, ...values) {
        let source = "";
        strings.forEach((part, i) => {
            source += part;
            if (i < values.length)
                source += `$p${i}`;
        });
        const params = {};
        values.forEach((v, i) => {
            params[`p${i}`] = v;
        });
        return this.query(source, params);
    }
    /** Best-effort close of the underlying connection when it exposes close/end. */
    async close() {
        const db = this.conn;
        const closer = db?.close ?? db?.end;
        if (typeof closer === "function") {
            const res = closer.call(db);
            if (res && typeof res.then === "function")
                await res;
        }
    }
}
function detectDriver(conn) {
    const c = conn;
    const has = (k) => typeof c?.[k] === "function";
    if (has("unsafe"))
        return "postgres.js";
    if (has("prepare") && !has("all"))
        return "better-sqlite3";
    if (has("all") && has("run"))
        return typeof c?.on === "function" ? "sqlite3" : "duckdb";
    if (has("query")) {
        if (has("promise") || c?.config != null)
            return "mysql2";
        if (has("connect") || c?.connectionParameters != null || c?.options != null) {
            return "pg";
        }
        return "mysql2";
    }
    throw new Error("[pipeql] Unsupported database driver: no recognizable query API. Pass { driver } to force a driver kind.");
}
/** Wrap any native database connection with automatic PipeQL compilation and dispatch. */
export function createPipeqlDriver(conn, options) {
    return new PipeqlDriver(conn, options);
}
