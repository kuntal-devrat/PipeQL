"""
PipeQL Python (PyO3) Benchmark

Measures compile latency for 1,000 read queries + 1,000 mutations
using the PyO3-backed native extension. Includes FFI overhead.

Also compares analyzer-catalog overhead across the three compile paths:
compile (no validation) vs compile_with_catalog (validation, catalog built
once) vs compile_with_schema (validation, catalog re-derived per call).
"""
import time
import sys
import os

# Ensure the Python package is importable
sys.path.insert(0, os.path.dirname(__file__))

from pipeql_python import compile as pipeql_compile
from pipeql_python import compile_with_catalog, compile_with_schema

READ_SHAPES = [
    "from orders | join customers on orders.customer_id == customers.id | filter orders.status == 'active' and orders.total >= $min_total | group [customer_id, region] (total = sum(orders.total), cnt = count(*)) | filter total > $threshold | select [customer_id, region, total, cnt] | sort [total desc, customer_id asc] | take 10",
    "from events | filter event_type in ['click', 'view', 'hover'] and user_id is not null | group [user_id, day] (views = count(*), last = max(timestamp)) | sort [views desc] | take 100 | select [user_id, day, views, last]",
    "from users u | join orders o on u.id == o.user_id | join payments p on o.id == p.order_id | filter u.age >= {low} and u.age <= {high} and p.amount > 0 | select [u.id, u.name, o.total, p.amount] | sort [o.total desc] | take 10",
    "from log_entries | filter level in ['error', 'warn'] and not (service == 'health') | group [service, date] (errors = count(*)) | filter errors > $min_errors | select [service, date, errors] | sort [date desc, errors desc] | take 50",
    "from products | filter category == 'electronics' | filter price >= $min_price and price <= $max_price | select [id, name, price] | sort [price asc] | take 20 | skip 40",
    "from sessions s | join users u on s.user_id == u.id | filter u.plan in ['pro', 'enterprise'] and s.duration >= $min_seconds | group [u.id] (total_duration = sum(s.duration), sessions = count(*)) | select [u.id, u.name, total_duration, sessions] | sort [total_duration desc] | take 10",
    "from inventory | filter stock <= $reorder_point and (warehouse == 'north' or warehouse == 'east') | select [sku, name, stock, warehouse] | sort [stock asc, warehouse asc]",
    "from transactions t | join accounts a on t.account_id == a.id | filter t.status == 'completed' and a.balance >= $min_balance | group [a.country] (revenue = sum(t.amount), count = count(*)) | filter count >= $min_count | select [a.country, revenue, count] | sort [revenue desc] | take 100",
]

MUTATION_SHAPES = [
    "into notes | insert [title = $title, content = $content, category = 'Personal', is_pinned = 0, created_at = CURRENT_TIMESTAMP]",
    "from notes | filter is_archived == 0 | filter id == $id | update [title = $title, is_pinned = 1, updated_at = CURRENT_TIMESTAMP]",
    "from notes | filter id == $id or is_archived == 1 | delete",
]


def build_corpus(n):
    all_shapes = READ_SHAPES + MUTATION_SHAPES
    queries = []
    for i in range(n):
        shape = all_shapes[i % len(all_shapes)]
        if "{low}" in shape:
            queries.append(shape.replace("{low}", str(i)).replace("{high}", str(i + 40)))
        else:
            queries.append(shape)
    return queries


def build_mutation_corpus(n):
    return [MUTATION_SHAPES[i % len(MUTATION_SHAPES)] for i in range(n)]


def benchmark_compile(queries, label):
    # Warmup
    pipeql_compile(queries[0], "postgres")

    times = []
    for q in queries:
        start = time.perf_counter()
        pipeql_compile(q, "postgres")
        elapsed = (time.perf_counter() - start) * 1000  # ms
        times.append(elapsed)

    times.sort()
    total = sum(times)
    mean = total / len(times)
    median = times[len(times) // 2]
    p95 = times[int(len(times) * 0.95)]
    p99 = times[int(len(times) * 0.99)]
    min_t = times[0]
    max_t = times[-1]

    print(f"\n=== {label} ({len(queries)} queries) ===")
    print(f"  Total:   {total:.2f} ms")
    print(f"  Mean:    {mean:.3f} ms/query")
    print(f"  Median:  {median:.3f} ms/query")
    print(f"  Min:     {min_t:.3f} ms")
    print(f"  Max:     {max_t:.3f} ms")
    print(f"  P95:     {p95:.3f} ms")
    print(f"  P99:     {p99:.3f} ms")

    return {"total": total, "mean": mean, "median": median, "p95": p95, "p99": p99}


# Schema + queries that pass analyzer validation in all three compile paths
# (avoids group aliases, which the catalog mode validates strictly).
CATALOG_SCHEMA = """\
table orders [id integer primary auto, customer_id integer, region string, status string, total float, user_id integer]
table customers [id integer primary auto, name string]
table users [id integer primary auto, name string, age integer, plan string]
table products [id integer primary auto, name string, price float, category string]
table inventory [id integer primary auto, sku string, name string, stock integer, warehouse string]
table transactions [id integer primary auto, account_id integer, status string, amount float]
table accounts [id integer primary auto, balance float, country string]
table notes [id integer primary auto, title string, content string, category string, is_pinned integer, is_archived integer, created_at timestamp, updated_at timestamp]
"""

CATALOG_QUERIES = [
    "from orders | join customers on orders.customer_id == customers.id | filter orders.status == 'active' and orders.total >= $min_total | select [orders.id, orders.total, customers.name] | sort [orders.total desc] | take 10",
    "from users | filter age >= $low and plan in ['pro', 'enterprise'] | select [id, name] | sort [name asc] | take 50",
    "from products | filter category == 'electronics' and price >= $min_price | select [id, name, price] | sort [price asc] | take 20",
    "from inventory | filter stock <= $reorder_point and (warehouse == 'north' or warehouse == 'east') | select [sku, name, stock, warehouse] | sort [stock asc]",
    "from transactions t | join accounts a on t.account_id == a.id | filter t.status == 'completed' and a.balance >= $min_balance | select [t.id, a.country, t.amount] | sort [t.amount desc] | take 100",
    "into notes | insert [title = $title, content = $content, category = 'Personal', is_pinned = 0]",
    "from notes | filter is_archived == 0 | filter id == $id | update [title = $title, is_pinned = 1]",
    "from notes | filter id == $id or is_archived == 1 | delete",
]


def benchmark_catalog_paths(reps=1000):
    """Compare compile / compile_with_catalog / compile_with_schema on one corpus."""
    from pipeql_python import catalog_from_schema

    # Derived once, outside the timed loop — the production pattern for the
    # pre-built catalog path.
    catalog = catalog_from_schema(CATALOG_SCHEMA)
    queries = CATALOG_QUERIES * (reps // len(CATALOG_QUERIES))

    def time_path(fn):
        fn(queries[0])  # warmup
        times = []
        for q in queries:
            start = time.perf_counter()
            fn(q)
            times.append((time.perf_counter() - start) * 1000)
        times.sort()
        return {
            "mean": sum(times) / len(times),
            "median": times[len(times) // 2],
            "p95": times[int(len(times) * 0.95)],
        }

    base = time_path(lambda q: pipeql_compile(q, "postgres"))
    with_catalog = time_path(lambda q: compile_with_catalog(q, "postgres", catalog))
    with_schema = time_path(lambda q: compile_with_schema(q, "postgres", CATALOG_SCHEMA))

    def row(name, r):
        ov = (r["mean"] - base["mean"]) / base["mean"] * 100 if base["mean"] else 0
        print(f"  {name:<28} mean {r['mean']:>8.3f} ms   median {r['median']:>8.3f} ms   "
              f"p95 {r['p95']:>8.3f} ms   vs compile {ov:+6.1f}%")

    print(f"\n=== Catalog overhead ({len(queries)} compiles per path) ===")
    row("compile", base)
    row("compile_with_catalog", with_catalog)
    row("compile_with_schema", with_schema)


def main():
    print("PipeQL Python (PyO3) Benchmark")
    print("=" * 40)

    read_queries = build_corpus(1000)
    mutation_queries = build_mutation_corpus(1000)

    read_result = benchmark_compile(read_queries, "Read Corpus (1000 queries)")
    mutation_result = benchmark_compile(mutation_queries, "Mutation Corpus (1000 queries)")

    # Single largest query
    largest = READ_SHAPES[0]
    start = time.perf_counter()
    pipeql_compile(largest, "postgres")
    single_time = (time.perf_counter() - start) * 1000
    print(f"\n=== Single Largest Query ===")
    print(f"  Time: {single_time:.3f} ms")

    print(f"\n=== Summary ===")
    print(f"  Read avg:     {read_result['total'] / 1000:.3f} ms/query")
    print(f"  Mutation avg: {mutation_result['total'] / 1000:.3f} ms/query")

    benchmark_catalog_paths()


if __name__ == "__main__":
    main()
