import assert from "node:assert/strict";
import { createPipeqlDriver } from "../src/driver.js";

// --- Fake sqlite3 (callback duck-typed API) -----------------------------
function fakeSqlite3() {
  const calls = [];
  const db = {
    calls,
    on() {},
    all(sql, params, cb) {
      calls.push(["all", sql, [...params]]);
      cb(null, [{ id: 1, name: "a" }]);
    },
    run(sql, params, cb) {
      calls.push(["run", sql, [...params]]);
      const ctx = { lastID: 7, changes: 1 };
      cb.call(ctx, null);
    },
  };
  return db;
}

// 1. Auto-detect sqlite3 + dialect inference; select -> .all()
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "sqlite3");
  assert.equal(db.dialect, "sqlite");

  const rows = await db.query("from users | filter id == $id | select [id, name]", { id: 42 });
  assert.deepEqual(rows, [{ id: 1, name: "a" }]);
  assert.equal(raw.calls[0][0], "all");
  assert.deepEqual(raw.calls[0][2], [42]);
}

// 2. Mutation -> .run(); execute() returns { lastId, changes }
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  const res = await db.execute("into notes | insert [title = $title]", { title: "Hi" });
  assert.equal(raw.calls[0][0], "run");
  assert.equal(res.lastId, 7);
  assert.equal(res.changes, 1);
}

// 3. query() auto-dispatches mutations too
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  const res = await db.query("from notes | filter id == $id | update [is_pinned = 1]", { id: 3 });
  assert.equal(raw.calls[0][0], "run");
  assert.deepEqual(res, { lastId: 7, changes: 1, rows: [] });
}

// 4. create_table (DDL) dispatches to .run(), never .all()
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  await db.query("table notes [id int primary auto]", {});
  assert.equal(raw.calls[0][0], "run");
}

// 5. Tagged template: interpolation -> $pN params, bound as args, never inlined
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  const user = "O'Reilly";
  const rows = await db.pipeql`from users | filter name == ${user} and active == ${1} | select [id]`;
  assert.deepEqual(rows, [{ id: 1, name: "a" }]);
  assert.equal(raw.calls[0][0], "all");
  assert.deepEqual(raw.calls[0][2], ["O'Reilly", 1]);
  assert.ok(!raw.calls[0][1].includes("O'Reilly"), "interpolation leaked into SQL");
}

// 6. better-sqlite3 (sync, prepare-only)
{
  const calls = [];
  const raw = {
    prepare(sql) {
      calls.push(["prepare", sql]);
      return {
        all: (...args) => {
          calls.push(["all", args]);
          return [{ ok: 1 }];
        },
        run: (...args) => {
          calls.push(["run", args]);
          return { lastInsertRowid: 9, changes: 2 };
        },
      };
    },
  };
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "better-sqlite3");
  assert.equal(db.dialect, "sqlite");
  const rows = await db.query("from t | select [ok]");
  assert.deepEqual(rows, [{ ok: 1 }]);
  const res = await db.execute("into t | insert [v = $v]", { v: 5 });
  assert.equal(res.lastId, 9);
  assert.equal(res.changes, 2);
}

// 7. pg (async client, $N placeholders, rows + rowCount)
{
  const calls = [];
  const raw = {
    connectionParameters: {},
    connect: () => {},
    async query(sql, args) {
      calls.push([sql, [...args]]);
      return { rows: [{ id: 2 }], rowCount: 1 };
    },
  };
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "pg");
  assert.equal(db.dialect, "postgres");
  const rows = await db.query("from t | filter id == $id | select [id]", { id: 2 });
  assert.deepEqual(rows, [{ id: 2 }]);
  assert.ok(calls[0][0].includes("$1"));
}

// 8. mysql2 (promise wrapper, [rows, fields])
{
  const calls = [];
  const raw = {
    config: {},
    query: () => {},
    promise: () => ({
      async query(sql, args) {
        calls.push([sql, [...args]]);
        return [[{ insertId: 11, affectedRows: 4 }], []];
      },
    }),
  };
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "mysql2");
  assert.equal(db.dialect, "mysql");
  const res = await db.execute("into t | insert [v = $v]", { v: 1 });
  assert.equal(res.lastId, 11);
  assert.equal(res.changes, 4);
  assert.ok(calls[0][0].includes("?"));
}

// 9. postgres.js (unsafe)
{
  const calls = [];
  const raw = {
    unsafe: async (sql, args) => {
      calls.push([sql, [...args]]);
      return [{ id: 3 }];
    },
  };
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "postgres.js");
  assert.equal(db.dialect, "postgres");
  const rows = await db.query("from t | select [id]");
  assert.deepEqual(rows, [{ id: 3 }]);
}

// 10. duckdb (all/run, no EventEmitter .on)
{
  const calls = [];
  const raw = {
    all(sql, params, cb) {
      calls.push(["all", sql]);
      cb(null, [{ x: 1 }]);
    },
    run(sql, params, cb) {
      calls.push(["run", sql]);
      cb(null);
    },
  };
  const db = createPipeqlDriver(raw);
  assert.equal(db.driver, "duckdb");
  assert.equal(db.dialect, "duckdb");
  const rows = await db.query("from t | select [x]");
  assert.deepEqual(rows, [{ x: 1 }]);
}

// 11. Explicit driver + dialect override bypasses detection
{
  const db = createPipeqlDriver({}, { driver: "sqlite3", dialect: "sqlite" });
  assert.equal(db.driver, "sqlite3");
  assert.equal(db.dialect, "sqlite");
}

// 12. Unsupported connection raises a clear error
assert.throws(() => createPipeqlDriver({ foo: 1 }), /Unsupported database driver/);

// 13. $data object expansion (insert) via execute()
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  const res = await db.execute("into notes | insert $data", {
    data: { title: "Hello", is_pinned: 1 },
  });
  assert.equal(raw.calls[0][0], "run");
  assert.ok(raw.calls[0][1].includes("(title, is_pinned)"), raw.calls[0][1]);
  assert.deepEqual(raw.calls[0][2], ["Hello", 1]);
  assert.equal(res.lastId, 7);
}

// 14. $data partial update (SET before WHERE ordering)
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  await db.execute("from notes | filter id == $id | update $data", {
    id: 3,
    data: { is_pinned: 1 },
  });
  assert.equal(raw.calls[0][0], "run");
  assert.ok(raw.calls[0][1].includes("SET is_pinned = ?"), raw.calls[0][1]);
  assert.deepEqual(raw.calls[0][2], [1, 3]);
}

// 15. $data expansion inside an existing bracket list (mixed)
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  await db.execute("into notes | insert [title = $title, $data]", {
    title: "T",
    data: { is_pinned: 1 },
  });
  assert.ok(raw.calls[0][1].includes("(title, is_pinned)"), raw.calls[0][1]);
  assert.deepEqual(raw.calls[0][2], ["T", 1]);
}

// 16. $data requires an object with at least one property
{
  const db = createPipeqlDriver(fakeSqlite3());
  await assert.rejects(() => db.execute("into notes | insert $data", {}), /\$data/);
  await assert.rejects(
    () => db.execute("into notes | insert $data", { data: {} }),
    /\$data/,
  );
}

// 17. insertAndFetch returns the created row (RETURNING *)
{
  const raw = {
    calls: [],
    on() {},
    all(sql, params, cb) {
      raw.calls.push(["all", sql, [...params]]);
      cb(null, [{ id: 5, title: "Hi", is_pinned: 1 }]);
    },
    run(sql, params, cb) {
      raw.calls.push(["run", sql, [...params]]);
      cb.call({ lastID: 5, changes: 1 }, null);
    },
  };
  const db = createPipeqlDriver(raw);
  const note = await db.insertAndFetch("into notes | insert $data", {
    title: "Hi",
    is_pinned: 1,
  });
  assert.deepEqual(note, { id: 5, title: "Hi", is_pinned: 1 });
  assert.ok(raw.calls[0][1].includes("RETURNING *"), raw.calls[0][1]);
  assert.deepEqual(raw.calls[0][2], ["Hi", 1]);
}

// 18. updateAndFetch on better-sqlite3 (sync)
{
  const raw = {
    prepare(sql) {
      return {
        all: (...args) => {
          assert.ok(sql.includes("RETURNING *"), sql);
          assert.deepEqual([...args], ["y", 1, 1]);
          return [{ id: 1, title: "y", is_pinned: 1 }];
        },
        run: () => ({ lastInsertRowid: 1, changes: 1 }),
      };
    },
  };
  const db = createPipeqlDriver(raw);
  const note = await db.updateAndFetch(
    "from notes | filter id == $id | update $data",
    { id: 1, data: { title: "y", is_pinned: 1 } },
  );
  assert.deepEqual(note, { id: 1, title: "y", is_pinned: 1 });
}

// 19. insertAndFetch on mysql2 falls back to run metadata (no RETURNING)
{
  const raw = {
    config: {},
    query: () => {},
    promise: () => ({
      async query(sql, args) {
        assert.ok(!/RETURNING/i.test(sql));
        return [[{ insertId: 9, affectedRows: 1 }], []];
      },
    }),
  };
  const db = createPipeqlDriver(raw);
  const res = await db.insertAndFetch("into notes | insert [title = $title]", {
    title: "Hi",
  });
  assert.deepEqual(res, { lastId: 9, changes: 1, rows: [] });
}

// 20. compile() exposes expanded args for $data sources
{
  const db = createPipeqlDriver(fakeSqlite3());
  const c = await db.compile("into notes | insert $data", {
    data: { title: "A", is_pinned: 0 },
  });
  assert.equal(c.statementType, "insert");
  assert.equal(c.isMutation, true);
  assert.deepEqual(c.args, ["A", 0]);
}

// 21. UNION queries return rows (regression: dispatched as mutation, rows dropped)
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  const rows = await db.query(
    "from active | select [id] | union from archived | select [id]",
  );
  assert.deepEqual(rows, [{ id: 1, name: "a" }]);
  assert.equal(raw.calls[0][0], "all", "union must dispatch to .all(), not .run()");
  assert.ok(raw.calls[0][1].includes("UNION"), raw.calls[0][1]);
}

// 22. Missing user param fails loudly (regression: silently bound its name)
{
  const db = createPipeqlDriver(fakeSqlite3());
  await assert.rejects(
    () => db.query("from users | filter id == $user_id | select [id]", { id: 42 }),
    /missing value for parameter 'user_id'/,
  );
}

// 23. close() method on db connection
{
  let closeCalled = false;
  const raw = {
    ...fakeSqlite3(),
    close() {
      closeCalled = true;
    }
  };
  const db = createPipeqlDriver(raw);
  await db.close();
  assert.ok(closeCalled, "close() should have been called");
}

// 24. end() method fallback on db connection
{
  let endCalled = false;
  const raw = {
    ...fakeSqlite3(),
    end() {
      endCalled = true;
    }
  };
  const db = createPipeqlDriver(raw);
  await db.close();
  assert.ok(endCalled, "end() should have been called");
}

// 25. async close() method on db connection
{
  let closeCalled = false;
  const raw = {
    ...fakeSqlite3(),
    async close() {
      closeCalled = true;
    }
  };
  const db = createPipeqlDriver(raw);
  await db.close();
  assert.ok(closeCalled, "async close() should have been called");
}

// 26. no close() or end() method on db connection
{
  const raw = fakeSqlite3();
  const db = createPipeqlDriver(raw);
  await db.close();
  assert.ok(true, "should not throw if close/end is missing");
}

console.log("all @pipeql/js/driver smoke tests passed");
