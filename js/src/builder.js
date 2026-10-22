/**
 * @pipeql/js/builder — fluent query builder for PipeQL.
 *
 * Composes a PipeQL **source string** stage by stage, then compiles it through
 * the same WASM facade as any hand-written query — so a builder query and a
 * literal string query are provably identical. No dual parser, no semantic
 * drift.
 *
 * Object inserts/updates (`insert`, `update`, `upsert`, `doUpdate`) accept
 * `{column: value}` objects and auto-generate `$b0`, `$b1`, ... bind
 * parameters — the `$data` ergonomics without a driver.
 *
 * ```ts
 * import { PipeQL } from "@pipeql/js/builder";
 *
 * const q = PipeQL.from("notes")
 *   .filter("is_archived == 0")
 *   .sort(["created_at desc"])
 *   .take(10);
 *
 * q.source();                      // "from notes | filter ... | take 10"
 * const { sql, params } = await q.compile("postgres");
 * q.values;                        // { b0: ..., b1: ... } for object inserts
 *
 * // Runs through any PipeqlDriver (duck-typed):
 * const rows = await db.query(q);
 * ```
 */
import { compile } from "./index.js";
function isPlainObject(v) {
    return v != null && typeof v === "object" && !Array.isArray(v);
}
function asList(cols) {
    if (Array.isArray(cols))
        return cols.join(", ");
    return String(cols);
}
function otherSource(other) {
    return other instanceof PipeQL ? other.source() : String(other);
}
/**
 * Fluent PipeQL query builder. Every stage method appends to the composed
 * source and returns `this` for chaining.
 */
export class PipeQL {
    constructor(source) {
        this._values = {};
        this._source = source;
    }
    /** Start a read pipeline: `from <table>`. */
    static from(table) {
        return new PipeQL(`from ${table}`);
    }
    /** Start an insert/upsert pipeline: `into <table>`. */
    static into(table) {
        return new PipeQL(`into ${table}`);
    }
    /** Start from an explicit PipeQL source string. */
    static raw(source) {
        return new PipeQL(source);
    }
    _stage(stage) {
        this._source += ` | ${stage}`;
        return this;
    }
    /** `| filter <expr>` */
    filter(expr) {
        return this._stage(`filter ${expr}`);
    }
    /** `| select [<cols>]` */
    select(cols) {
        return this._stage(`select [${asList(cols)}]`);
    }
    /** `| derive [<cols>]` */
    derive(cols) {
        return this._stage(`derive [${asList(cols)}]`);
    }
    /** `| sort [<cols>]` */
    sort(cols) {
        return this._stage(`sort [${asList(cols)}]`);
    }
    /** `| take <n>` */
    take(n) {
        return this._stage(`take ${n}`);
    }
    /** `| skip <n>` */
    skip(n) {
        return this._stage(`skip ${n}`);
    }
    /** `| join <table> on <on>` */
    join(table, on) {
        return this._stage(`join ${table} on ${on}`);
    }
    /** `| left join <table> on <on>` */
    leftJoin(table, on) {
        return this._stage(`left join ${table} on ${on}`);
    }
    /** `| right join <table> on <on>` */
    rightJoin(table, on) {
        return this._stage(`right join ${table} on ${on}`);
    }
    /** `| full join <table> on <on>` */
    fullJoin(table, on) {
        return this._stage(`full join ${table} on ${on}`);
    }
    /** `| inner join <table> on <on>` */
    innerJoin(table, on) {
        return this._stage(`inner join ${table} on ${on}`);
    }
    /** `| group [<cols>] (<aggs>)` */
    group(cols, aggs) {
        return this._stage(`group [${asList(cols)}] (${aggs})`);
    }
    /** `| union <other>` where `other` is a source string or builder. */
    union(other) {
        return this._stage(`union ${otherSource(other)}`);
    }
    /** `| union all <other>` */
    unionAll(other) {
        return this._stage(`union all ${otherSource(other)}`);
    }
    /** Append an explicit stage string. */
    rawStage(stage) {
        return this._stage(stage);
    }
    /** `| insert [...]` with auto-generated `$b0, $b1, ...` params. */
    insert(values) {
        return this._assignments("insert", values);
    }
    /** `| update [...]` (requires a preceding filter stage). */
    update(values) {
        return this._assignments("update", values);
    }
    /** `| update all [...]` — explicit opt-in for a full-table update that
     * bypasses the filter guard. */
    updateAll(values) {
        return this._assignments("update all", values);
    }
    /** `| delete` */
    delete() {
        return this._stage("delete");
    }
    /** `| delete all` — explicit opt-in for a full-table delete that bypasses
     * the filter guard. */
    deleteAll() {
        return this._stage("delete all");
    }
    /** `| upsert [...]` */
    upsert(values) {
        return this._assignments("upsert", values);
    }
    /** `| conflict [<cols>]` */
    conflict(cols) {
        return this._stage(`conflict [${asList(cols)}]`);
    }
    /** `| do update [...]` */
    doUpdate(values) {
        return this._assignments("do update", values);
    }
    _assignments(kind, values) {
        let body;
        if (isPlainObject(values)) {
            body = [];
            let valCount = Object.keys(this._values).length;
            for (const [key, val] of Object.entries(values)) {
                const pname = `b${valCount++}`;
                this._values[pname] = val;
                body.push(`${key} = $${pname}`);
            }
        }
        else if (Array.isArray(values)) {
            body = values.map(String);
        }
        else {
            body = [String(values)];
        }
        return this._stage(`${kind} [${body.join(", ")}]`);
    }
    /** The composed PipeQL source string. */
    source() {
        return this._source;
    }
    /** Bound values from object inserts/updates, keyed by `$bN` name. */
    get values() {
        return { ...this._values };
    }
    /** Compile through the standard facade. */
    async compile(dialect = "postgres") {
        const result = await compile(this._source, dialect);
        return { ...result, values: { ...this._values } };
    }
    toString() {
        return this._source;
    }
}
