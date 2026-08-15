/**
 * PipeQL JS/TS (WASM) Benchmark
 *
 * Measures compile latency for 1,000 read queries + 1,000 mutations
 * using the WASM-backed SDK. Includes FFI/WASM-call overhead.
 *
 * Also compares analyzer-catalog overhead across the three compile paths:
 * compile (no validation) vs compileWithCatalog (validation, catalog built
 * once) vs compileWithSchema (validation, catalog re-derived per call).
 */
import {
  catalogFromSchema,
  compile,
  compileWithCatalog,
  compileWithSchema,
  initWasm,
} from "./src/index.js";

const READ_SHAPES = [
  "from orders | join customers on orders.customer_id == customers.id | filter orders.status == 'active' and orders.total >= $min_total | group [customer_id, region] (total = sum(orders.total), cnt = count(*)) | filter total > $threshold | select [customer_id, region, total, cnt] | sort [total desc, customer_id asc] | take 10",
  "from events | filter event_type in ['click', 'view', 'hover'] and user_id is not null | group [user_id, day] (views = count(*), last = max(timestamp)) | sort [views desc] | take 100 | select [user_id, day, views, last]",
  "from users u | join orders o on u.id == o.user_id | join payments p on o.id == p.order_id | filter u.age >= {low} and u.age <= {high} and p.amount > 0 | select [u.id, u.name, o.total, p.amount] | sort [o.total desc] | take 10",
  "from log_entries | filter level in ['error', 'warn'] and not (service == 'health') | group [service, date] (errors = count(*)) | filter errors > $min_errors | select [service, date, errors] | sort [date desc, errors desc] | take 50",
  "from products | filter category == 'electronics' | filter price >= $min_price and price <= $max_price | select [id, name, price] | sort [price asc] | take 20 | skip 40",
  "from sessions s | join users u on s.user_id == u.id | filter u.plan in ['pro', 'enterprise'] and s.duration >= $min_seconds | group [u.id] (total_duration = sum(s.duration), sessions = count(*)) | select [u.id, u.name, total_duration, sessions] | sort [total_duration desc] | take 10",
  "from inventory | filter stock <= $reorder_point and (warehouse == 'north' or warehouse == 'east') | select [sku, name, stock, warehouse] | sort [stock asc, warehouse asc]",
  "from transactions t | join accounts a on t.account_id == a.id | filter t.status == 'completed' and a.balance >= $min_balance | group [a.country] (revenue = sum(t.amount), count = count(*)) | filter count >= $min_count | select [a.country, revenue, count] | sort [revenue desc] | take 100",
];

const MUTATION_SHAPES = [
  "into notes | insert [title = $title, content = $content, category = 'Personal', is_pinned = 0, created_at = CURRENT_TIMESTAMP]",
  "from notes | filter is_archived == 0 | filter id == $id | update [title = $title, is_pinned = 1, updated_at = CURRENT_TIMESTAMP]",
  "from notes | filter id == $id or is_archived == 1 | delete",
];

function buildCorpus(n) {
  const all = [...READ_SHAPES, ...MUTATION_SHAPES];
  const queries = [];
  for (let i = 0; i < n; i++) {
    const shape = all[i % all.length];
    if (shape.includes("{low}")) {
      queries.push(shape.replace("{low}", String(i)).replace("{high}", String(i + 40)));
    } else {
      queries.push(shape);
    }
  }
  return queries;
}

function buildMutationCorpus(n) {
  const queries = [];
  for (let i = 0; i < n; i++) {
    queries.push(MUTATION_SHAPES[i % MUTATION_SHAPES.length]);
  }
  return queries;
}

async function benchmarkCompile(queries, label) {
  // Warmup: compile first query once
  await compile(queries[0], "postgres");

  const times = [];
  for (const q of queries) {
    const start = performance.now();
    await compile(q, "postgres");
    times.push(performance.now() - start);
  }

  times.sort((a, b) => a - b);
  const total = times.reduce((a, b) => a + b, 0);
  const mean = total / times.length;
  const median = times[Math.floor(times.length / 2)];
  const p95 = times[Math.floor(times.length * 0.95)];
  const p99 = times[Math.floor(times.length * 0.99)];
  const min = times[0];
  const max = times[times.length - 1];

  console.log(`\n=== ${label} (${queries.length} queries) ===`);
  console.log(`  Total:   ${total.toFixed(2)} ms`);
  console.log(`  Mean:    ${mean.toFixed(3)} ms/query`);
  console.log(`  Median:  ${median.toFixed(3)} ms/query`);
  console.log(`  Min:     ${min.toFixed(3)} ms`);
  console.log(`  Max:     ${max.toFixed(3)} ms`);
  console.log(`  P95:     ${p95.toFixed(3)} ms`);
  console.log(`  P99:     ${p99.toFixed(3)} ms`);

  return { total, mean, median, p95, p99, min, max };
}

// Schema + queries that pass analyzer validation in all three compile paths
// (avoids group aliases, which the catalog mode validates strictly).
const CATALOG_SCHEMA = `
table orders [id integer primary auto, customer_id integer, region string, status string, total float, user_id integer]
table customers [id integer primary auto, name string]
table users [id integer primary auto, name string, age integer, plan string]
table products [id integer primary auto, name string, price float, category string]
table inventory [id integer primary auto, sku string, name string, stock integer, warehouse string]
table transactions [id integer primary auto, account_id integer, status string, amount float]
table accounts [id integer primary auto, balance float, country string]
table notes [id integer primary auto, title string, content string, category string, is_pinned integer, is_archived integer, created_at timestamp, updated_at timestamp]
`;

const CATALOG_QUERIES = [
  "from orders | join customers on orders.customer_id == customers.id | filter orders.status == 'active' and orders.total >= $min_total | select [orders.id, orders.total, customers.name] | sort [orders.total desc] | take 10",
  "from users | filter age >= $low and plan in ['pro', 'enterprise'] | select [id, name] | sort [name asc] | take 50",
  "from products | filter category == 'electronics' and price >= $min_price | select [id, name, price] | sort [price asc] | take 20",
  "from inventory | filter stock <= $reorder_point and (warehouse == 'north' or warehouse == 'east') | select [sku, name, stock, warehouse] | sort [stock asc]",
  "from transactions t | join accounts a on t.account_id == a.id | filter t.status == 'completed' and a.balance >= $min_balance | select [t.id, a.country, t.amount] | sort [t.amount desc] | take 100",
  "into notes | insert [title = $title, content = $content, category = 'Personal', is_pinned = 0]",
  "from notes | filter is_archived == 0 | filter id == $id | update [title = $title, is_pinned = 1]",
  "from notes | filter id == $id or is_archived == 1 | delete",
];

async function benchmarkCatalogPaths(reps = 1000) {
  // Derived once, outside the timed loop — the production pattern for the
  // pre-built catalog path.
  const catalog = await catalogFromSchema(CATALOG_SCHEMA);
  const queries = [];
  for (let i = 0; i < reps; i++) queries.push(CATALOG_QUERIES[i % CATALOG_QUERIES.length]);

  async function timePath(fn) {
    await fn(queries[0]); // warmup
    const times = [];
    for (const q of queries) {
      const start = performance.now();
      await fn(q);
      times.push(performance.now() - start);
    }
    times.sort((a, b) => a - b);
    const mean = times.reduce((a, b) => a + b, 0) / times.length;
    return {
      mean,
      median: times[Math.floor(times.length / 2)],
      p95: times[Math.floor(times.length * 0.95)],
    };
  }

  const base = await timePath((q) => compile(q, "postgres"));
  const withCatalog = await timePath((q) => compileWithCatalog(q, catalog, "postgres"));
  const withSchema = await timePath((q) => compileWithSchema(q, CATALOG_SCHEMA, "postgres"));

  function row(name, r) {
    const ov = base.mean ? ((r.mean - base.mean) / base.mean) * 100 : 0;
    console.log(
      `  ${name.padEnd(28)} mean ${r.mean.toFixed(3).padStart(8)} ms   median ${r.median.toFixed(3).padStart(8)} ms   p95 ${r.p95.toFixed(3).padStart(8)} ms   vs compile ${ov >= 0 ? "+" : ""}${ov.toFixed(1)}%`,
    );
  }

  console.log(`\n=== Catalog overhead (${queries.length} compiles per path) ===`);
  row("compile", base);
  row("compileWithCatalog", withCatalog);
  row("compileWithSchema", withSchema);
}

async function main() {
  console.log("PipeQL JS/TS (WASM) Benchmark");
  console.log("=".repeat(40));

  await initWasm();

  const readQueries = buildCorpus(1000);
  const mutationQueries = buildMutationCorpus(1000);

  const readResult = await benchmarkCompile(readQueries, "Read Corpus (1000 queries)");
  const mutationResult = await benchmarkCompile(mutationQueries, "Mutation Corpus (1000 queries)");

  // Single largest query
  const largest = READ_SHAPES[0];
  const start = performance.now();
  await compile(largest, "postgres");
  const singleTime = performance.now() - start;
  console.log(`\n=== Single Largest Query ===`);
  console.log(`  Time: ${singleTime.toFixed(3)} ms`);

  // Per-query average
  console.log(`\n=== Summary ===`);
  console.log(`  Read avg:     ${(readResult.total / 1000).toFixed(3)} ms/query`);
  console.log(`  Mutation avg: ${(mutationResult.total / 1000).toFixed(3)} ms/query`);

  await benchmarkCatalogPaths();
}

main().catch(console.error);
