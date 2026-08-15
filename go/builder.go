package pipeql

// Fluent query builder for PipeQL.
//
// Composes a PipeQL source string stage by stage, then compiles it through the
// same C-ABI facade as any hand-written query — so a builder query and a
// literal string query are provably identical. No dual parser, no semantic
// drift.
//
// Object inserts/updates (Insert, Update, Upsert, DoUpdate) accept
// map[string]any values and auto-generate $b0, $b1, ... bind parameters — the
// $data ergonomics without a driver.
//
// Usage:
//
//	q := From("notes").
//		Filter("is_archived == 0").
//		Sort([]string{"created_at desc"}).
//		Take(10)
//
//	res, err := q.Compile("postgres")
//	fmt.Println(res.SQL)

import (
	"fmt"
	"sort"
	"strings"
)

// Pairs is an ordered column → value list for object inserts/updates.
// Go maps iterate in random order, so use Pairs (or sortable maps) when the
// generated assignment order matters.
type Pairs []Pair

// Pair is a single column → value assignment.
type Pair struct {
	Key string
	Val any
}

// PairsOf builds an ordered Pairs list from alternating key/value arguments.
// Panics on an odd argument count.
func PairsOf(kvs ...any) Pairs {
	if len(kvs)%2 != 0 {
		panic("pipeql: PairsOf requires an even number of arguments (key, value, ...)")
	}
	p := make(Pairs, 0, len(kvs)/2)
	for i := 0; i < len(kvs); i += 2 {
		p = append(p, Pair{Key: kvs[i].(string), Val: kvs[i+1]})
	}
	return p
}

// Builder is a fluent PipeQL query builder.
//
// Every stage method appends to the composed source and returns the same
// Builder for chaining. Use Source for the PipeQL text, Compile to compile it,
// or Values for object-insert bound values.
type Builder struct {
	source string
	values []kv
}

// Query is an alias for Builder for compatibility across naming conventions.
type Query = Builder

type kv struct {
	key string
	val any
}

// From starts a read pipeline: "from <table>".
func From(table string) *Builder {
	return &Builder{source: fmt.Sprintf("from %s", table)}
}

// Into starts an insert/upsert pipeline: "into <table>".
func Into(table string) *Builder {
	return &Builder{source: fmt.Sprintf("into %s", table)}
}

// Raw starts from an explicit PipeQL source string.
func Raw(source string) *Builder {
	return &Builder{source: source}
}

func (b *Builder) stage(stage string) *Builder {
	b.source += " | " + stage
	return b
}

// Filter adds "| filter <expr>".
func (b *Builder) Filter(expr string) *Builder {
	return b.stage(fmt.Sprintf("filter %s", expr))
}

// Select adds "| select [<cols>]".
func (b *Builder) Select(cols any) *Builder {
	return b.stage(fmt.Sprintf("select [%s]", list(cols)))
}

// Derive adds "| derive [<cols>]".
func (b *Builder) Derive(cols any) *Builder {
	return b.stage(fmt.Sprintf("derive [%s]", list(cols)))
}

// Sort adds "| sort [<cols>]".
func (b *Builder) Sort(cols any) *Builder {
	return b.stage(fmt.Sprintf("sort [%s]", list(cols)))
}

// Take adds "| take <n>".
func (b *Builder) Take(n int) *Builder {
	return b.stage(fmt.Sprintf("take %d", n))
}

// Skip adds "| skip <n>".
func (b *Builder) Skip(n int) *Builder {
	return b.stage(fmt.Sprintf("skip %d", n))
}

// Join adds "| join <table> on <on>".
func (b *Builder) Join(table, on string) *Builder {
	return b.stage(fmt.Sprintf("join %s on %s", table, on))
}

// LeftJoin adds "| left join <table> on <on>".
func (b *Builder) LeftJoin(table, on string) *Builder {
	return b.stage(fmt.Sprintf("left join %s on %s", table, on))
}

// RightJoin adds "| right join <table> on <on>".
func (b *Builder) RightJoin(table, on string) *Builder {
	return b.stage(fmt.Sprintf("right join %s on %s", table, on))
}

// FullJoin adds "| full join <table> on <on>".
func (b *Builder) FullJoin(table, on string) *Builder {
	return b.stage(fmt.Sprintf("full join %s on %s", table, on))
}

// InnerJoin adds "| inner join <table> on <on>".
func (b *Builder) InnerJoin(table, on string) *Builder {
	return b.stage(fmt.Sprintf("inner join %s on %s", table, on))
}

// Group adds "| group [<cols>] (<aggs>)".
func (b *Builder) Group(cols any, aggs string) *Builder {
	return b.stage(fmt.Sprintf("group [%s] (%s)", list(cols), aggs))
}

// Union adds "| union <other>" where other is a source string or Builder.
func (b *Builder) Union(other any) *Builder {
	return b.stage(fmt.Sprintf("union %s", sourceOf(other)))
}

// UnionAll adds "| union all <other>".
func (b *Builder) UnionAll(other any) *Builder {
	return b.stage(fmt.Sprintf("union all %s", sourceOf(other)))
}

// RawStage appends an explicit stage string.
func (b *Builder) RawStage(stage string) *Builder {
	return b.stage(stage)
}

// Insert adds "| insert [...]" with auto-generated $b0, $b1, ... params.
func (b *Builder) Insert(values any) *Builder {
	return b.assignments("insert", values)
}

// Update adds "| update [...]" (requires a preceding filter stage).
func (b *Builder) Update(values any) *Builder {
	return b.assignments("update", values)
}

// UpdateAll adds "| update all [...]" — explicit opt-in for a full-table
// update that bypasses the filter guard.
func (b *Builder) UpdateAll(values any) *Builder {
	return b.assignments("update all", values)
}

// Delete adds "| delete".
func (b *Builder) Delete() *Builder {
	return b.stage("delete")
}

// DeleteAll adds "| delete all" — explicit opt-in for a full-table delete
// that bypasses the filter guard.
func (b *Builder) DeleteAll() *Builder {
	return b.stage("delete all")
}

// Upsert adds "| upsert [...]".
func (b *Builder) Upsert(values any) *Builder {
	return b.assignments("upsert", values)
}

// Conflict adds "| conflict [<cols>]".
func (b *Builder) Conflict(cols any) *Builder {
	return b.stage(fmt.Sprintf("conflict [%s]", list(cols)))
}

// DoUpdate adds "| do update [...]".
func (b *Builder) DoUpdate(values any) *Builder {
	return b.assignments("do update", values)
}

func (b *Builder) assignments(kind string, values any) *Builder {
	var body []string
	switch v := values.(type) {
	case map[string]any:
		// Sort keys for deterministic SQL (Go maps iterate randomly).
		keys := make([]string, 0, len(v))
		for key := range v {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		for _, key := range keys {
			pname := fmt.Sprintf("b%d", len(b.values))
			b.values = append(b.values, kv{key, v[key]})
			body = append(body, fmt.Sprintf("%s = $%s", key, pname))
		}
	case Pairs:
		for _, p := range v {
			pname := fmt.Sprintf("b%d", len(b.values))
			b.values = append(b.values, kv{p.Key, p.Val})
			body = append(body, fmt.Sprintf("%s = $%s", p.Key, pname))
		}
	case string:
		body = []string{v}
	case []string:
		body = v
	default:
		body = []string{fmt.Sprintf("%v", values)}
	}
	return b.stage(fmt.Sprintf("%s [%s]", kind, strings.Join(body, ", ")))
}

// Source returns the composed PipeQL source string.
func (b *Builder) Source() string {
	return b.source
}

// Values returns bound values from object inserts/updates, keyed by $bN name.
func (b *Builder) Values() map[string]any {
	m := make(map[string]any, len(b.values))
	for i, kv := range b.values {
		m[fmt.Sprintf("b%d", i)] = kv.val
	}
	return m
}

// Compile compiles the composed source through the C-ABI facade.
// Object insert values ($b0, $b1, ...) are merged into the result.
func (b *Builder) Compile(dialect string) (*Result, error) {
	res, err := Compile(b.source, dialect)
	if err != nil {
		return nil, err
	}
	// Merge bound values into result params if present
	if len(b.values) > 0 {
		for i := range b.values {
			key := fmt.Sprintf("b%d", i)
			found := false
			for _, p := range res.Params {
				if p == key {
					found = true
					break
				}
			}
			if !found {
				res.Params = append(res.Params, key)
			}
		}
	}
	return res, nil
}

func (b *Builder) String() string {
	return b.source
}

func list(cols any) string {
	switch v := cols.(type) {
	case string:
		return v
	case []string:
		return strings.Join(v, ", ")
	default:
		return fmt.Sprintf("%v", cols)
	}
}

func sourceOf(other any) string {
	if b, ok := other.(*Builder); ok {
		return b.source
	}
	return fmt.Sprintf("%v", other)
}
