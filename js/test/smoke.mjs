import assert from "node:assert/strict";
import {
  catalogFromSchema,
  compile,
  compileWithCatalog,
  compileWithSchema,
  parse,
  pipeql,
  supportedDialects,
  version,
} from "../src/index.js";

await compile("from t", "postgres");

// 1. Basic compile
{
  const { sql, params, parameterCount } = await compile(
    "from users | filter age >= $min_age and status == 'active' | select [id, name] | sort [name asc] | take 10",
    "postgres",
  );
  assert.ok(sql.includes("SELECT id, name FROM users"));
  assert.ok(sql.includes("age >= $1"));
  // String literals become bind params too (PRD: 100% parameter extraction).
  assert.deepEqual(params, ["min_age", "active"]);
  assert.equal(parameterCount, 2);
}

// 2. Dialects
{
  const sqlite = await compile("from users | filter id == $id | take 5", "sqlite");
  assert.ok(sqlite.sql.includes("LIMIT 5"));
  assert.ok(sqlite.sql.includes("id = ?"));
  const mysql = await compile("from users | filter id == $id | take 5", "mysql");
  assert.ok(mysql.sql.includes("LIMIT 5"));
}

// 2b. Statement metadata drives driver dispatch (no SQL prefix sniffing)
{
  const select = await compile("from users | filter id == $id | select [id]", "postgres");
  assert.equal(select.statementType, "select");
  assert.equal(select.isMutation, false);

  const insert = await compile("into notes | insert [title = $t, is_pinned = 0]", "sqlite");
  assert.equal(insert.statementType, "insert");
  assert.equal(insert.isMutation, true);

  const update = await compile("from notes | filter id == $id | update [is_pinned = 1]", "sqlite");
  assert.equal(update.statementType, "update");
  assert.equal(update.isMutation, true);

  const del = await compile("from notes | filter id == $id | delete", "sqlite");
  assert.equal(del.statementType, "delete");
  assert.equal(del.isMutation, true);

  const ddl = await compile("table notes [id int primary auto]", "sqlite");
  assert.equal(ddl.statementType, "create_table");
  assert.equal(ddl.isMutation, false);

  const upsert = await compile(
    "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
    "postgres",
  );
  assert.equal(upsert.statementType, "upsert");
  assert.equal(upsert.isMutation, true);
  assert.ok(upsert.sql.includes("ON CONFLICT (email) DO UPDATE SET name = $1"));

  const union = await compile(
    "from active_users | select [id, name] | union from archived_users | select [id, name]",
    "postgres",
  );
  assert.equal(union.statementType, "union");
  assert.equal(union.isMutation, false);
  assert.ok(union.sql.includes("UNION"));
  assert.ok(!union.sql.includes("UNION ALL"));

  const unionAll = await compile(
    "from active_users | select [id, name] | union all from archived_users | select [id, name]",
    "postgres",
  );
  assert.ok(unionAll.sql.includes("UNION ALL"));
}

// 3. Tagged template with interpolation -> parameters, never inline
{
  const q = pipeql`from users | filter age >= ${18} and plan == ${"pro"} | select [id]`;
  const { sql, params, values } = await q.compile("postgres");
  assert.ok(sql.includes("age >= $1"));
  assert.ok(sql.includes("plan = $2"));
  assert.deepEqual(params, ["p0", "p1"]);
  assert.deepEqual(values, [18, "pro"]);
}

// 3b. Subquery (IN subquery)
{
  const result = await compile(
    "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
    "postgres",
  );
  assert.ok(result.sql.includes("IN (SELECT id FROM customers"));
  assert.ok(result.sql.includes("WHERE (region = $1)"));
  assert.deepEqual(result.params, ["EU"]);
}

// 4. Catalog validation catches unknown columns
{
  const catalog = {
    users: { name: "users", columns: [{ name: "id", ty: "Integer" }] },
  };
  const ok = await compileWithCatalog("from users | select [id]", catalog);
  assert.ok(ok.sql.includes("SELECT id FROM users"));
  await assert.rejects(
    () => compileWithCatalog("from users | select [nope]", catalog),
    /nope|Unknown column/,
  );
}

// 4b. Schema-derived catalog + one-call compileWithSchema
{
  const schema =
    "table users [id integer primary auto, name string not null]\n\ntable posts [id integer primary auto, user_id integer]";
  const catalog = await catalogFromSchema(schema);
  assert.deepEqual(catalog.users.columns, [
    { name: "id", ty: "Integer" },
    { name: "name", ty: "String" },
  ]);
  assert.deepEqual(Object.keys(catalog).sort(), ["posts", "users"]);

  const ok = await compileWithSchema("from users | filter id == $id", schema);
  assert.ok(ok.sql.includes("WHERE (id = $1)"), ok.sql);
  assert.deepEqual(ok.params, ["id"]);

  await assert.rejects(
    () => compileWithSchema("from users | filter nme == $x", schema),
    /nme|Unknown column/,
  );
  await assert.rejects(
    () => compileWithSchema("from users | select [id]", "from users | select [id]"),
    /table/,
  );
  await assert.rejects(
    () => catalogFromSchema("table t [id integer]\ntable t [id integer]"),
    /duplicate table/,
  );

  // Timestamp columns are now natively typed as Timestamp across all layers.
  const ts = await catalogFromSchema("table events [id integer, at timestamp]");
  assert.deepEqual(ts.events.columns, [
    { name: "id", ty: "Integer" },
    { name: "at", ty: "Timestamp" },
  ]);
  await compileWithSchema("from events | filter at == $t", "table events [id integer, at timestamp]");

  // Derivation is memoized, and callers get defensive copies: mutating the
  // returned catalog must not poison the cache for the next caller.
  const c1 = await catalogFromSchema("table t [id integer]");
  const c2 = await catalogFromSchema("table t [id integer]");
  assert.deepEqual(c1, c2);
  assert.notEqual(c1, c2);
  c1.t.columns.push({ name: "junk", ty: "String" });
  const c3 = await catalogFromSchema("table t [id integer]");
  assert.deepEqual(c3.t.columns, [{ name: "id", ty: "Integer" }]);

  // Errors are not cached: the same bad schema raises every time.
  for (let i = 0; i < 2; i++) {
    await assert.rejects(
      () => catalogFromSchema("table t [id integer]\ntable t [id integer]"),
      /duplicate table/,
    );
  }
}

// 5. Errors carry actionable hints
{
  await assert.rejects(() => compile("from users | filter", "postgres"), /hint|expected/i);
}

// 6. AST parse
{
  const ast = await parse("from users | filter id == $x | select [id]");
  assert.equal(ast.source.name.name, "users");
  assert.ok(Array.isArray(ast.steps));
}

// 7. Introspection
{
  const dialects = await supportedDialects();
  assert.ok(dialects.includes("postgres"));
  assert.match(await version(), /^\d+\.\d+\.\d+$/);
}

console.log("all @pipeql/js smoke tests passed");
