use pipeql_core::*;

/// Compile a PipeQL source with a given dialect and return (sql, params).
fn compile(src: &str, dialect: &str) -> (String, Vec<String>) {
    let mut parser = Parser::new(src).expect("lexer should not fail");
    let pipeline = parser.parse_pipeline().expect("parser should not fail");
    let d = get_dialect(dialect).expect("dialect should be valid");
    d.compile(&pipeline).expect("codegen should not fail")
}

fn compile_pg(src: &str) -> (String, Vec<String>) {
    compile(src, "postgres")
}

#[test]
fn test_prd_example_1_filter_derive_select() {
    let (sql, params) = compile_pg(
        "from employees
        | filter status == 'active' and salary >= 50000
        | derive [
            tax = salary * 0.20,
            net_pay = salary - tax
          ]
        | select [id, name, department, net_pay]
        | sort [net_pay desc]
        | take 10",
    );

    assert!(sql.contains("FROM employees"));
    assert!(sql.contains("WHERE ((status = $1) AND (salary >= 50000))"));
    assert!(sql.contains("(salary - (salary * 0.2)) AS net_pay"));
    assert!(sql.contains("ORDER BY net_pay DESC"));
    assert!(sql.contains("LIMIT 10"));
    assert_eq!(params, vec!["active".to_string()]);
}

#[test]
fn test_prd_example_2_join_dot_json() {
    let (sql, _params) = compile_pg(
        "from posts as p
        | join left users as u on p.author_id == u.id
        | filter u.is_verified == true
        | select [
            p.id,
            p.title,
            u.profile.name as author_name,
            p.created_at
          ]",
    );

    assert!(sql.contains("FROM posts AS p"));
    assert!(sql.contains("LEFT JOIN users AS u ON (p.author_id = u.id)"));
    assert!(sql.contains("WHERE (u.is_verified = true)"));
    assert!(sql.contains("u.profile->>'name' AS author_name"));
    assert!(sql.contains("SELECT p.id, p.title,"));
}

#[test]
fn test_prd_example_3_aggregation_group() {
    let (sql, params) = compile_pg(
        "from orders
        | filter created_at >= '2026-01-01'
        | group [customer_id] (
            total_spent = sum(amount),
            order_count = count(id),
            avg_value = avg(amount)
          )
        | filter total_spent > 1000
        | sort [total_spent desc]",
    );

    assert!(sql.contains("SELECT customer_id, SUM(amount) AS total_spent"));
    assert!(sql.contains("COUNT(id) AS order_count"));
    assert!(sql.contains("AVG(amount) AS avg_value"));
    assert!(sql.contains("FROM orders"));
    assert!(sql.contains("WHERE (created_at >= $1)"));
    assert!(sql.contains("GROUP BY customer_id"));
    assert!(sql.contains("HAVING (sum(amount) > 1000)"));
    assert!(sql.contains("ORDER BY total_spent DESC"));
    assert_eq!(params, vec!["2026-01-01".to_string()]);
}

#[test]
fn test_simple_select() {
    let (sql, params) = compile_pg("from users | select [id, name]");
    assert_eq!(sql, "SELECT id, name FROM users;");
    assert!(params.is_empty());
}

#[test]
fn test_select_with_alias() {
    let (sql, _) = compile_pg("from users | select [id, first_name as fname]");
    assert!(sql.contains("first_name AS fname"));
}

#[test]
fn test_derive_and_select() {
    let (sql, _) = compile_pg("from users | derive [full_age = age + 1] | select [id, full_age]");
    assert!(sql.contains("(age + 1) AS full_age"));
}

#[test]
fn test_filter_param_extraction() {
    let (sql, params) = compile_pg("from users | filter id == $user_id and name == ${full_name}");
    assert!(sql.contains("id = $1"));
    assert!(sql.contains("name = $2"));
    assert_eq!(params, vec!["user_id".to_string(), "full_name".to_string()]);
}

#[test]
fn test_string_literal_becomes_param() {
    let (sql, params) = compile_pg("from users | filter name == 'Alice'");
    assert!(sql.contains("name = $1"));
    assert_eq!(params, vec!["Alice".to_string()]);
}

#[test]
fn test_sort_directions() {
    let (sql, _) = compile_pg("from users | sort [name asc, age desc]");
    assert!(sql.contains("ORDER BY name ASC, age DESC"));
}

#[test]
fn test_take_skip() {
    let (sql, _) = compile_pg("from users | skip 5 | take 10");
    assert!(sql.contains("LIMIT 10"));
    assert!(sql.contains("OFFSET 5"));
}

#[test]
fn test_all_join_types() {
    for (join_kw, expected) in [
        ("inner", "INNER JOIN"),
        ("left", "LEFT JOIN"),
        ("right", "RIGHT JOIN"),
        ("full", "FULL OUTER JOIN"),
    ] {
        let (sql, _) = compile_pg(&format!("from a | join {join_kw} b on a.id == b.a_id"));
        assert!(
            sql.contains(expected),
            "join {join_kw} should produce {expected}, got: {sql}"
        );
    }
}

#[test]
fn test_default_join_is_inner() {
    let (sql, _) = compile_pg("from a | join b on a.id == b.a_id");
    assert!(sql.contains("INNER JOIN"));
}

#[test]
fn test_all_join_types_sql_style_prefix() {
    // `left join b on ...` (SQL order) — the spelling the docs advertise.
    for (join_kw, expected) in [
        ("inner", "INNER JOIN"),
        ("left", "LEFT JOIN"),
        ("right", "RIGHT JOIN"),
        ("full", "FULL OUTER JOIN"),
    ] {
        let (sql, _) = compile_pg(&format!("from a | {join_kw} join b on a.id == b.a_id"));
        assert!(
            sql.contains(expected),
            "{join_kw} join should produce {expected}, got: {sql}"
        );
    }
}

#[test]
fn test_left_join_sql_style_prefix() {
    let (sql, _) = compile_pg("from notes | left join archive on notes.id == archive.note_id");
    assert!(sql.contains("LEFT JOIN archive ON (notes.id = archive.note_id)"));
}

#[test]
fn test_left_join_sql_style_prefix_with_alias() {
    let (sql, _) =
        compile_pg("from posts as p | left join users as u on p.author_id == u.id");
    assert!(sql.contains("FROM posts AS p"));
    assert!(sql.contains("LEFT JOIN users AS u ON (p.author_id = u.id)"));
}

#[test]
fn test_outer_join_types_all_dialects() {
    for dialect in ["postgres", "sqlite", "duckdb"] {
        for (join_kw, expected) in [
            ("left", "LEFT JOIN"),
            ("right", "RIGHT JOIN"),
            ("full", "FULL OUTER JOIN"),
        ] {
            let (sql, _) =
                compile(&format!("from a | {join_kw} join b on a.id == b.a_id"), dialect);
            assert!(
                sql.contains(expected),
                "{dialect} {join_kw} join should produce {expected}, got: {sql}"
            );
        }
    }
    // MySQL does not support FULL OUTER JOIN — verify it errors
    let result = std::panic::catch_unwind(|| {
        compile("from a | full join b on a.id == b.a_id", "mysql");
    });
    assert!(result.is_err(), "MySQL FULL OUTER JOIN should fail");
}

#[test]
fn test_in_literal_list_parenthesized() {
    let (sql, params) = compile_pg("from t | filter id in (1, 2, 3)");
    assert!(sql.contains("(id IN (1, 2, 3))"));
    assert!(params.is_empty());
}

#[test]
fn test_in_string_list_parenthesized() {
    let (sql, params) = compile_pg("from t | filter status in ('a', 'b')");
    assert!(sql.contains("(status IN ($1, $2))"));
    assert_eq!(params, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_not_in_parenthesized() {
    let (sql, _) = compile_pg("from t | filter id not in (1, 2, 3)");
    assert!(sql.contains("(id NOT IN (1, 2, 3))"));
}

#[test]
fn test_in_parenthesized_with_params() {
    let (sql, params) = compile_pg("from t | filter region in ($a, $b)");
    assert!(sql.contains("(region IN ($1, $2))"));
    assert_eq!(params, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_in_parenthesized_all_dialects() {
    for dialect in ["postgres", "sqlite", "duckdb", "mysql"] {
        let (sql, params) = compile("from t | filter id in (1, 2, 3)", dialect);
        assert!(sql.contains("(id IN (1, 2, 3))"), "{dialect}: {sql}");
        assert!(params.is_empty(), "{dialect}");
    }
}

#[test]
fn test_left_join_with_in_parens_combined() {
    let (sql, _) = compile_pg(
        "from orders | left join customers on orders.customer_id == customers.id \
         | filter customers.region in ('EU', 'APAC') | select [orders.id]",
    );
    assert!(sql.contains("LEFT JOIN customers ON (orders.customer_id = customers.id)"));
    assert!(sql.contains("(customers.region IN ($1, $2))"));
    assert!(sql.contains("SELECT orders.id"));
}

#[test]
fn test_group_by_multiple_columns() {
    let (sql, _) = compile_pg("from sales | group [region, product] (total = sum(amount))");
    assert!(sql.contains("GROUP BY region, product"));
    assert!(sql.contains("SELECT region, product, SUM(amount) AS total"));
}

#[test]
fn test_filter_before_group_is_where() {
    let (sql, _) =
        compile_pg("from sales | filter region == 'west' | group [region] (t = sum(amount))");
    assert!(sql.contains("WHERE (region = $1)"));
    assert!(!sql.contains("HAVING (region"));
}

#[test]
fn test_filter_after_group_is_having() {
    let (sql, _) = compile_pg("from sales | group [region] (t = sum(amount)) | filter t > 100");
    assert!(sql.contains("HAVING (sum(amount) > 100)"));
    assert!(!sql.contains("WHERE (t > 100)"));
}

#[test]
fn test_multiple_having_conditions() {
    let (sql, _) =
        compile_pg("from sales | group [region] (t = sum(amount)) | filter t > 100 and t < 1000");
    assert!(sql.contains("HAVING ((sum(amount) > 100) AND (sum(amount) < 1000))"));
}

#[test]
fn test_boolean_logic_precedence() {
    let (sql, _) = compile_pg("from t | filter a == 1 or b == 2 and c == 3");
    // AND binds tighter than OR
    assert!(sql.contains("(a = 1) OR ((b = 2) AND (c = 3))"));
}

#[test]
fn test_parenthesized_expression() {
    let (sql, _) = compile_pg("from t | filter (a == 1 or b == 2) and c == 3");
    assert!(sql.contains("((a = 1) OR (b = 2)) AND (c = 3)"));
}

#[test]
fn test_arithmetic_ops() {
    let (sql, _) = compile_pg("from t | derive [x = a + b * c - d / e]");
    assert!(sql.contains("(a + (b * c)) - (d / e)"));
}

#[test]
fn test_comparison_operators() {
    let cases = [
        ("a == 1", "a = 1"),
        ("a != 1", "a <> 1"),
        ("a < 1", "a < 1"),
        ("a <= 1", "a <= 1"),
        ("a > 1", "a > 1"),
        ("a >= 1", "a >= 1"),
    ];
    for (pipeql, sql) in cases {
        let (out, _) = compile_pg(&format!("from t | filter {pipeql}"));
        assert!(
            out.contains(sql),
            "expected {pipeql} to compile to {sql}, got: {out}"
        );
    }
}

#[test]
fn test_function_calls() {
    let (sql, _) =
        compile_pg("from users | filter length(name) > 3 | select [upper(name) as uname]");
    assert!(sql.contains("length(name) > 3"));
    assert!(sql.contains("upper(name) AS uname"));
}

#[test]
fn test_literals() {
    let (sql, params) = compile_pg(
        "from t | filter a == 42 and b == 3.14 and c == 'str' and d == true and e == null",
    );
    assert!(sql.contains("a = 42"));
    assert!(sql.contains("b = 3.14"));
    assert!(sql.contains("c = $1"));
    assert!(sql.contains("d = true"));
    assert!(sql.contains("e = NULL"));
    assert_eq!(params, vec!["str".to_string()]);
}

#[test]
fn test_newline_as_pipe_separator() {
    let (sql, _) = compile_pg(
        "from users
        filter age > 18
        select [id]",
    );
    assert!(sql.contains("WHERE (age > 18)"));
    assert!(sql.contains("SELECT id"));
}

#[test]
fn test_mixed_newline_and_pipe() {
    let (sql, _) = compile_pg(
        "from users
        | filter age > 18
        select [id]
        | take 5",
    );
    assert!(sql.contains("WHERE (age > 18)"));
    assert!(sql.contains("LIMIT 5"));
}

#[test]
fn test_comments_are_ignored() {
    let (sql, _) = compile_pg(
        "-- leading comment
        from users -- trailing comment
        | filter age > 18
        | select [id] -- another comment",
    );
    assert!(sql.contains("WHERE (age > 18)"));
    assert!(sql.contains("SELECT id"));
}

#[test]
fn test_table_alias() {
    let (sql, _) = compile_pg("from users as u | select [u.id, u.name]");
    assert!(sql.contains("FROM users AS u"));
    assert!(sql.contains("u.id, u.name"));
}

#[test]
fn test_case_insensitive_keywords() {
    let (sql, _) = compile_pg("FROM USERS | FILTER AGE > 18 | SELECT [ID]");
    assert!(sql.contains("WHERE (AGE > 18)"));
    assert!(sql.contains("SELECT ID"));
}

#[test]
fn test_sqlite_uses_question_marks() {
    let (sql, params) = compile("from users | filter name == $name", "sqlite");
    assert!(sql.contains("name = ?"));
    assert_eq!(params, vec!["name".to_string()]);
}

#[test]
fn test_duckdb_uses_question_marks() {
    let (sql, params) = compile("from sales | filter region == $r", "duckdb");
    assert!(sql.contains("region = ?"));
    assert_eq!(params, vec!["r".to_string()]);
}

#[test]
fn test_mysql_uses_question_marks() {
    let (sql, params) = compile("from sales | filter region == $r", "mysql");
    assert!(sql.contains("region = ?"));
    assert_eq!(params, vec!["r".to_string()]);
}

#[test]
fn test_invalid_dialect_errors() {
    let err = match get_dialect("oracle") {
        Ok(_) => panic!("oracle should not be a valid dialect"),
        Err(e) => e,
    };
    assert!(matches!(err, CodegenError::UnsupportedDialect(_)));
}

#[test]
fn test_parameterized_strings_in_all_dialects() {
    for dialect in ["postgres", "sqlite", "duckdb", "mysql"] {
        let (sql, params) = compile("from t | filter status == 'active'", dialect);
        assert_eq!(params, vec!["active".to_string()]);
        assert!(
            sql.contains("status = ") || sql.contains("status =?"),
            "dialect {dialect} should contain status = marker: {sql}"
        );
    }
}

#[test]
fn test_select_star() {
    let (sql, _) = compile_pg("from users | select [*]");
    assert_eq!(sql, "SELECT * FROM users;");
}

#[test]
fn test_select_star_with_columns() {
    let (sql, _) = compile_pg("from users | select [*, name]");
    assert_eq!(sql, "SELECT *, name FROM users;");
}

#[test]
fn test_no_select_defaults_to_star() {
    let (sql, _) = compile_pg("from users | filter age > 18");
    assert!(sql.starts_with("SELECT * FROM users"));
}

#[test]
fn test_derive_without_select() {
    let (sql, _) = compile_pg("from items | derive [total = price * qty]");
    assert!(sql.contains("SELECT *, (price * qty) AS total FROM items"));
}

#[test]
fn test_derive_multiple_without_select() {
    let (sql, _) = compile_pg("from items | derive [total = price * qty, tax = total * 0.1]");
    assert!(sql.contains("(price * qty) AS total"));
    assert!(sql.contains("((price * qty) * 0.1) AS tax"));
}

#[test]
fn test_derive_chained_reference_in_select() {
    let (sql, _) = compile_pg("from t | derive [x = a + 1, y = x * 2] | select [y]");
    assert!(sql.contains("((a + 1) * 2) AS y"));
}

#[test]
fn test_take_without_skip() {
    let (sql, _) = compile_pg("from users | take 5");
    assert!(sql.contains("LIMIT 5"));
    assert!(!sql.contains("OFFSET"));
}

#[test]
fn test_skip_without_take() {
    let (sql, _) = compile_pg("from users | skip 10");
    assert!(sql.contains("OFFSET 10"));
    assert!(!sql.contains("LIMIT"));
}

#[test]
fn test_sort_defaults_to_asc() {
    let (sql, _) = compile_pg("from users | sort [name]");
    assert!(sql.contains("ORDER BY name ASC"));
}

#[test]
fn test_sort_multiple_mixed_directions() {
    let (sql, _) = compile_pg("from t | sort [a asc, b desc, c]");
    assert!(sql.contains("ORDER BY a ASC, b DESC, c ASC"));
}

#[test]
fn test_join_with_parameter() {
    let (sql, params) = compile_pg("from a | join b on a.id == $join_id");
    assert!(sql.contains("ON (a.id = $1)"));
    assert_eq!(params, vec!["join_id".to_string()]);
}

#[test]
fn test_nested_function_calls() {
    let (sql, params) =
        compile_pg("from t | select [coalesce(a, b, 'fallback') as fb, abs(round(x)) as ar]");
    assert!(sql.contains("coalesce(a, b, $1) AS fb"));
    assert!(sql.contains("abs(round(x)) AS ar"));
    assert_eq!(params, vec!["fallback".to_string()]);
}

#[test]
fn test_zero_arg_function() {
    let (sql, _) = compile_pg("from t | select [now() as ts]");
    assert!(sql.contains("now() AS ts"));
}

#[test]
fn test_deep_json_path() {
    let (sql, _) = compile_pg("from t | select [meta.info.tags.primary]");
    assert!(sql.contains("meta.info->>'tags'->>'primary'"));
}

#[test]
fn test_json_path_in_derive() {
    let (sql, _) =
        compile_pg("from users as u | derive [display = u.profile.name] | select [display]");
    assert!(sql.contains("u.profile->>'name' AS display"));
}

#[test]
fn test_multiple_filters_before_group() {
    let (sql, _) = compile_pg(
        "from sales | filter region == 'west' | filter amount > 10 | group [region] (t = sum(amount))",
    );
    assert!(sql.contains("WHERE (region = $1) AND (amount > 10)"));
    assert!(!sql.contains("HAVING"));
}

#[test]
fn test_group_without_aggregates() {
    let (sql, _) = compile_pg("from t | group [a] ()");
    assert!(sql.contains("SELECT a FROM t"));
    assert!(sql.contains("GROUP BY a"));
}

#[test]
fn test_group_multiple_columns_multiple_aggs() {
    let (sql, _) =
        compile_pg("from orders | group [region, year] (total = sum(amount), items = count(*))");
    assert!(sql.contains("SELECT region, year, SUM(amount) AS total, COUNT(*) AS items"));
    assert!(sql.contains("GROUP BY region, year"));
}

#[test]
fn test_string_params_in_select() {
    let (sql, params) = compile_pg("from t | select [concat(first, '-', last) as combined]");
    assert!(sql.contains("concat(first, $1, last) AS combined"));
    assert_eq!(params, vec!["-".to_string()]);
}

#[test]
fn test_param_in_derive_and_sort() {
    let (sql, params) = compile_pg("from t | derive [x = bonus * $multiplier] | sort [x desc]");
    assert!(sql.contains("(bonus * $1) AS x"));
    assert!(sql.contains("ORDER BY x DESC"));
    assert_eq!(params, vec!["multiplier".to_string()]);
}

#[test]
fn test_two_string_params_question_style() {
    let (sql, params) = compile("from t | filter a == 'x' and b == 'y'", "sqlite");
    assert!(sql.contains("(a = ?) AND (b = ?)"));
    assert_eq!(params, vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn test_mysql_take_skip() {
    let (sql, _) = compile("from users | skip 5 | take 10", "mysql");
    assert!(sql.contains("LIMIT 10"));
    assert!(sql.contains("OFFSET 5"));
}

#[test]
fn test_all_dialects_full_pipeline() {
    for dialect in ["postgres", "sqlite", "duckdb", "mysql"] {
        let (sql, params) = compile(
            "from users | filter status == 'active' | select [id, name] | sort [name asc] | take 10",
            dialect,
        );
        assert!(
            sql.contains("SELECT id, name FROM users"),
            "{dialect}: {sql}"
        );
        assert!(sql.contains("ORDER BY name ASC"), "{dialect}: {sql}");
        assert!(sql.contains("LIMIT 10"), "{dialect}: {sql}");
        assert_eq!(params, vec!["active".to_string()], "{dialect}");
    }
}

#[test]
fn test_missing_pipe_uses_newline() {
    let (sql, _) = compile_pg(
        "from users
        select [id]
        take 3",
    );
    assert!(sql.contains("SELECT id FROM users"));
    assert!(sql.contains("LIMIT 3"));
}

#[test]
fn test_parse_error_unbalanced_bracket() {
    let mut parser = Parser::new("from users | select [id").expect("lexer should not fail");
    let err = parser.parse_pipeline();
    assert!(err.is_err(), "unbalanced bracket should fail to parse");
}

#[test]
fn test_parse_error_unknown_pipeline_keyword() {
    let mut parser = Parser::new("from users | explode [id]").expect("lexer should not fail");
    let err = parser.parse_pipeline();
    assert!(
        err.is_err(),
        "unknown pipeline keyword should fail to parse"
    );
}

#[test]
fn test_aggregate_in_select_without_group() {
    let (sql, _) = compile_pg("from t | select [sum(amount) as total]");
    assert!(sql.contains("sum(amount) AS total"));
}

#[test]
fn test_boolean_expressions_in_select() {
    let (sql, params) = compile_pg("from t | select [is_active, score >= 100 as passed]");
    assert!(sql.contains("(score >= 100) AS passed"));
    assert!(params.is_empty());
}

#[test]
fn test_float_and_negative_arithmetic() {
    let (sql, _) = compile_pg("from t | derive [net = price - discount]");
    assert!(sql.contains("(price - discount) AS net"));
}

#[test]
fn test_group_sort_having_take_combined() {
    let (sql, _) = compile_pg(
        "from orders | group [customer_id] (t = sum(amount)) | filter t > 500 | sort [t desc] | take 3",
    );
    assert!(sql.contains("GROUP BY customer_id"));
    assert!(sql.contains("HAVING (sum(amount) > 500)"));
    assert!(sql.contains("ORDER BY t DESC"));
    assert!(sql.contains("LIMIT 3"));
}

#[test]
fn test_not_operator() {
    let (sql, _) = compile_pg("from t | filter not active");
    assert!(sql.contains("NOT (active)"));

    let (sql, _) = compile_pg("from t | filter not a == 1");
    assert!(sql.contains("NOT ((a = 1))"));

    let (sql, _) = compile_pg("from t | filter not a == 1 and b == 2");
    assert!(sql.contains("(NOT ((a = 1)) AND (b = 2))"));
}

#[test]
fn test_is_null_operator() {
    let (sql, _) = compile_pg("from t | filter email is null");
    assert!(sql.contains("(email IS NULL)"));

    let (sql, _) = compile_pg("from t | filter email is not null");
    assert!(sql.contains("(email IS NOT NULL)"));
}

#[test]
fn test_in_operator() {
    let (sql, params) = compile_pg("from t | filter status in ['a', 'b']");
    assert!(sql.contains("(status IN ($1, $2))"));
    assert_eq!(params, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_not_in_operator() {
    let (sql, params) = compile_pg("from t | filter id not in [1, 2, 3]");
    assert!(sql.contains("(id NOT IN (1, 2, 3))"));
    assert!(params.is_empty());
}

#[test]
fn test_in_with_named_params() {
    let (sql, params) = compile_pg("from t | filter region in [$a, $b]");
    assert!(sql.contains("(region IN ($1, $2))"));
    assert_eq!(params, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn test_combined_operators() {
    let (sql, _) =
        compile_pg("from t | filter a == 1 and (b is null or c in ['x', 'y']) and not d");
    assert!(sql.contains("(b IS NULL)"));
    assert!(sql.contains("(c IN ($1, $2))"));
    assert!(sql.contains("NOT (d)"));
}

#[test]
fn test_postgres_param_dedup() {
    let (sql, params) = compile_pg("from t | filter a == $x and b == $x and c == 'v'");
    // The same param $x reuses $1; the string literal is a new slot.
    assert!(sql.contains("a = $1"));
    assert!(sql.contains("b = $1"));
    assert!(sql.contains("c = $2"));
    assert_eq!(params, vec!["x".to_string(), "v".to_string()]);
}

#[test]
fn test_question_style_params_are_positional() {
    let (sql, params) = compile("from t | filter a == $x and b == $x", "sqlite");
    assert!(sql.contains("a = ?) AND (b = ?"));
    assert_eq!(params, vec!["x".to_string(), "x".to_string()]);
}

#[test]
fn test_comments_preserved_in_ast() {
    let mut parser =
        Parser::new("-- header comment\nfrom users -- table comment\n| filter age > 18")
            .expect("lexer should not fail");
    let pipeline = parser.parse_pipeline().expect("parser should not fail");
    assert_eq!(pipeline.comments.len(), 2);
    assert_eq!(pipeline.comments[0].text, "header comment");
    assert_eq!(pipeline.comments[1].text, "table comment");
    assert_eq!(pipeline.comments[0].span.start, 0);
}

#[test]
fn test_parse_error_has_suggestion() {
    let mut parser = Parser::new("from users | explode [id]").expect("lexer should not fail");
    let err = parser.parse_pipeline().unwrap_err();
    assert!(err[0].suggestion.is_some(), "suggestion should be present");
    assert!(err[0].suggestion.as_ref().unwrap().contains("filter"));
}

#[test]
fn test_catalog_validates_columns() {
    use pipeql_core::{Catalog, ColumnMeta, TableMeta};
    let mut catalog = Catalog::new();
    catalog.add_table(TableMeta {
        name: "users".into(),
        columns: vec![ColumnMeta {
            name: "id".into(),
            ty: pipeql_core::ValueType::Integer,
        }],
    });

    let mut parser = Parser::new("from users | filter nope > 1").expect("lexer should not fail");
    let pipeline = parser.parse_pipeline().expect("parser should not fail");
    let d = get_dialect("postgres").unwrap();
    let err = d
        .compile_with_catalog(&pipeline, Some(&catalog))
        .unwrap_err();
    assert!(matches!(err, pipeql_core::CodegenError::Analysis(_)));
}

#[test]
fn test_analyzer_dedups_and_reports_occurrences() {
    use pipeql_core::Analyzer;
    let mut parser =
        Parser::new("from t | filter a == $x and b == $x").expect("lexer should not fail");
    let pipeline = parser.parse_pipeline().expect("parser should not fail");
    let analysis = Analyzer::new(None).analyze(&pipeline).unwrap();
    assert_eq!(analysis.param_count(), 1);
    assert_eq!(analysis.param_map[0].occurrences.len(), 2);
}

#[test]
fn test_single_equals_is_equality() {
    let (sql, params) =
        compile_pg("from employees | filter status = 'active' and salary >= 50000 | select [id]");
    assert!(sql.contains("WHERE ((status = $1) AND (salary >= 50000))"));
    assert_eq!(params, vec!["active".to_string()]);
}

#[test]
fn test_angle_bracket_neq() {
    let (sql, _) = compile_pg("from t | filter status <> 'cancelled'");
    assert!(sql.contains("status <> $1"));
}

#[test]
fn test_bare_alias_from_and_join() {
    let (sql, _) = compile_pg(
        "from posts p
        | join users u on p.author_id == u.id
        | select [p.id, u.name]",
    );
    assert!(sql.contains("FROM posts AS p"));
    assert!(sql.contains("JOIN users AS u ON (p.author_id = u.id)"));
    assert!(sql.contains("SELECT p.id, u.name"));
}

#[test]
fn test_leftover_tokens_after_pipeline_error() {
    let mut parser =
        Parser::new("from t | filter status == 'active' dangling").expect("lexer should not fail");
    let err = parser.parse_pipeline();
    assert!(
        err.is_err(),
        "leftover tokens after the pipeline must surface as an error, not be truncated"
    );
    let message = err.unwrap_err()[0].message.clone();
    assert!(message.contains("after pipeline"));
}

#[test]
fn test_bare_alias_must_be_fully_consumed() {
    // Regression guard: previously a bare alias stopped the pipeline silently.
    let mut parser = Parser::new("from users u | filter age > 18").expect("lexer should not fail");
    let pipeline = parser.parse_pipeline().expect("parser should not fail");
    assert_eq!(
        pipeline.source.alias.as_ref().map(|a| a.name.as_str()),
        Some("u")
    );
    let (sql, _) = compile_pg("from users u | filter age > 18");
    assert!(sql.contains("FROM users AS u"));
    assert!(sql.contains("WHERE (age > 18)"));
}

#[path = "../benches/corpus.rs"]
mod corpus;

#[test]
fn test_bench_corpus_compiles_all_dialects() {
    let queries = corpus::queries();
    assert_eq!(queries.len(), 1_000, "corpus must stay at 1,000 queries");
    for q in &queries {
        for dialect in ["postgres", "sqlite", "duckdb", "mysql"] {
            pipeql_core::api::compile(q, dialect)
                .unwrap_or_else(|e| panic!("corpus query failed for {dialect}: {e}\nquery: {q}"));
        }
    }
}

#[test]
fn test_mutation_corpus_compiles_all_dialects() {
    let queries = corpus::mutation_queries();
    assert_eq!(
        queries.len(),
        1_000,
        "mutation corpus must stay at 1,000 queries"
    );
    for q in &queries {
        for dialect in ["postgres", "sqlite", "duckdb", "mysql"] {
            let c = pipeql_core::api::compile(q, dialect)
                .unwrap_or_else(|e| panic!("mutation failed for {dialect}: {e}\nquery: {q}"));
            assert!(!c.sql.is_empty());
            // Every mutation query must have its values parameterized.
            assert!(
                !c.params.is_empty(),
                "mutation produced no parameters:\n{}",
                q
            );
        }
    }
}

/// Compile any PipeQL statement (read pipeline, insert, or DDL) via the facade.
fn compile_stmt(src: &str, dialect: &str) -> (String, Vec<String>) {
    let c = pipeql_core::api::compile(src, dialect).expect("compile should not fail");
    (c.sql, c.params)
}

#[test]
fn test_prd_v2_insert_postgres() {
    let (sql, params) = compile_stmt(
        "into notes
        | insert [
            title = $title,
            content = $content,
            category = 'Personal',
            is_pinned = 0
          ]",
        "postgres",
    );
    assert_eq!(
        sql,
        "INSERT INTO notes (title, content, category, is_pinned) VALUES ($1, $2, $3, $4) RETURNING *;"
    );
    assert_eq!(params, vec!["title", "content", "Personal", "0"]);
}

#[test]
fn test_prd_v2_insert_question_style() {
    let (sql, params) = compile_stmt(
        "into notes
        | insert [
            title = $title,
            content = $content,
            category = 'Personal',
            is_pinned = 0
          ]",
        "sqlite",
    );
    assert_eq!(
        sql,
        "INSERT INTO notes (title, content, category, is_pinned) VALUES (?, ?, ?, ?);"
    );
    assert_eq!(params, vec!["title", "content", "Personal", "0"]);
}

#[test]
fn test_prd_v2_update_question_style() {
    let (sql, params) = compile_stmt(
        "from notes
        | filter id == $id and is_archived == 0
        | update [
            title = $title,
            is_pinned = 1,
            updated_at = CURRENT_TIMESTAMP
          ]",
        "sqlite",
    );
    assert_eq!(
        sql,
        "UPDATE notes\nSET title = ?, is_pinned = ?, updated_at = CURRENT_TIMESTAMP\nWHERE ((id = ?) AND (is_archived = ?));"
    );
    assert_eq!(params, vec!["title", "1", "id", "0"]);
}

#[test]
fn test_prd_v2_update_postgres() {
    let (sql, params) = compile_stmt(
        "from notes
        | filter id == $id and is_archived == 0
        | update [
            title = $title,
            is_pinned = 1,
            updated_at = CURRENT_TIMESTAMP
          ]",
        "postgres",
    );
    assert_eq!(
        sql,
        "UPDATE notes\nSET title = $1, is_pinned = $2, updated_at = CURRENT_TIMESTAMP\nWHERE ((id = $3) AND (is_archived = $4));"
    );
    assert_eq!(params, vec!["title", "1", "id", "0"]);
}

#[test]
fn test_prd_v2_delete_postgres() {
    let (sql, params) = compile_stmt(
        "from notes
        | filter id == $id or is_archived == 1
        | delete",
        "postgres",
    );
    assert_eq!(
        sql,
        "DELETE FROM notes\nWHERE ((id = $1) OR (is_archived = $2));"
    );
    assert_eq!(params, vec!["id", "1"]);
}

#[test]
fn test_prd_v2_delete_question_style() {
    let (sql, params) = compile_stmt(
        "from notes
        | filter id == $id or is_archived == 1
        | delete",
        "sqlite",
    );
    assert_eq!(
        sql,
        "DELETE FROM notes\nWHERE ((id = ?) OR (is_archived = ?));"
    );
    assert_eq!(params, vec!["id", "1"]);
}

#[test]
fn test_prd_v2_table_sqlite() {
    let (sql, params) = compile_stmt(
        "table notes [
          id int primary auto,
          title string not null,
          content string not null,
          category string default 'Personal',
          is_pinned int default 0,
          created_at timestamp default current_timestamp
        ]",
        "sqlite",
    );
    assert_eq!(
        sql,
        "CREATE TABLE IF NOT EXISTS notes (\n  id INTEGER PRIMARY KEY AUTOINCREMENT,\n  title TEXT NOT NULL,\n  content TEXT NOT NULL,\n  category TEXT DEFAULT 'Personal',\n  is_pinned INTEGER DEFAULT 0,\n  created_at DATETIME DEFAULT CURRENT_TIMESTAMP\n);"
    );
    assert!(params.is_empty());
}

#[test]
fn test_prd_v2_table_postgres() {
    let (sql, _) = compile_stmt("table notes [id int primary auto]", "postgres");
    assert_eq!(
        sql,
        "CREATE TABLE IF NOT EXISTS notes (\n  id INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY\n);"
    );
}

#[test]
fn test_prd_v2_table_duckdb_mysql() {
    let (sql, _) = compile_stmt(
        "table notes [id int primary auto, title string not null, is_pinned bool default 0]",
        "duckdb",
    );
    assert_eq!(
        sql,
        "CREATE TABLE IF NOT EXISTS notes (\n  id INTEGER PRIMARY KEY GENERATED BY DEFAULT AS IDENTITY,\n  title VARCHAR NOT NULL,\n  is_pinned BOOLEAN DEFAULT 0\n);"
    );

    let (sql, _) = compile_stmt(
        "table notes [id int primary auto, title string not null, is_pinned bool default 0]",
        "mysql",
    );
    assert_eq!(
        sql,
        "CREATE TABLE IF NOT EXISTS notes (\n  id INT PRIMARY KEY AUTO_INCREMENT,\n  title VARCHAR(255) NOT NULL,\n  is_pinned BOOLEAN DEFAULT 0\n);"
    );
}

#[test]
fn test_update_multiple_filter_steps_are_anded() {
    let (sql, params) = compile_stmt(
        "from notes | filter is_archived == 0 | filter id == 0 | update [title = $t]",
        "postgres",
    );
    // PostgreSQL dedups repeated values to the same $N.
    assert_eq!(
        sql,
        "UPDATE notes\nSET title = $1\nWHERE (is_archived = $2) AND (id = $2);"
    );
    assert_eq!(params, vec!["t", "0"]);
}

#[test]
fn test_mutation_requires_preceding_filter() {
    // Documented safety rule: update/delete need a preceding filter step.
    for (src, verb) in [
        ("from notes | delete", "delete"),
        ("from notes | update [title = $t]", "update"),
    ] {
        let err = pipeql_core::api::compile(src, "postgres").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains(&format!("'{verb}' requires a preceding 'filter' stage")),
            "{msg}"
        );
    }

    // Filtered forms still compile.
    let (sql, _) = compile_stmt("from notes | filter id == $id | delete", "postgres");
    assert!(sql.contains("DELETE FROM notes"));
    let (sql, _) = compile_stmt("from notes | filter id == $id | update [title = $t]", "postgres");
    assert!(sql.contains("UPDATE notes"));
}

#[test]
fn test_mutation_all_escape_hatch() {
    // `delete all` / `update all [...]` explicitly opt in to full-table
    // operations and bypass the filter guard.
    let (sql, _) = compile_stmt("from notes | delete all", "sqlite");
    assert_eq!(sql, "DELETE FROM notes;");
    let (sql, _) = compile_stmt("from notes | update all [title = $t]", "postgres");
    assert_eq!(sql, "UPDATE notes\nSET title = $1;");
    // A filter combined with `all` still emits the WHERE clause.
    let (sql, _) = compile_stmt("from notes | filter id == $id | delete all", "sqlite");
    assert!(sql.contains("DELETE FROM notes"));
    assert!(sql.contains("WHERE"));
}

#[test]
fn test_mutation_rejects_non_filter_steps() {
    // With a filter present, non-filter steps before the mutation are still
    // rejected (codegen-level ordering rule).
    let err = pipeql_core::api::compile(
        "from notes | filter a == 1 | take 5 | delete",
        "postgres",
    )
    .unwrap_err();
    assert!(format!("{err}").contains("Only filter steps are permitted"));
}

#[test]
fn test_insert_null_is_inlined() {
    let (sql, params) = compile_stmt("into t | insert [a = NULL, b = $x]", "postgres");
    assert_eq!(sql, "INSERT INTO t (a, b) VALUES (NULL, $1) RETURNING *;");
    assert_eq!(params, vec!["x"]);
}

#[test]
fn test_parse_statement_variants() {
    // Read pipelines stay Pipeline statements; mutations parse into their own
    // statement types.
    let stmt = pipeql_core::api::parse_statement("from users | filter age > 18").unwrap();
    assert!(matches!(stmt, pipeql_core::Statement::Pipeline(_)));

    let stmt = pipeql_core::api::parse_statement("into notes | insert [title = $t]").unwrap();
    match stmt {
        pipeql_core::Statement::Insert(insert) => assert_eq!(insert.table.name, "notes"),
        _ => panic!("expected Insert statement"),
    }

    let stmt = pipeql_core::api::parse_statement("table notes [id int]").unwrap();
    match stmt {
        pipeql_core::Statement::CreateTable(create) => assert_eq!(create.name.name, "notes"),
        _ => panic!("expected CreateTable statement"),
    }
}

#[test]
fn test_subquery_many_parameters_postgres_no_collision() {
    // Outer query has $p1 and $p2. Subquery has 12 parameters ($s1 through $s12).
    // Test verifies that $1 replacement in the subquery never collides with $10, $11, $12.
    let src = "from orders | filter user_id in (from users | filter p1 == $s1 and p2 == $s2 and p3 == $s3 and p4 == $s4 and p5 == $s5 and p6 == $s6 and p7 == $s7 and p8 == $s8 and p9 == $s9 and p10 == $s10 and p11 == $s11 and p12 == $s12 | select [id]) and status == $p_status";
    let res = pipeql_core::api::compile(src, "postgres").unwrap();
    
    // Check that $12 is present and not corrupted to $120 or similar
    assert!(res.sql.contains("$12"), "SQL: {}", res.sql);
    assert!(res.sql.contains("$13"), "SQL: {}", res.sql);
    assert_eq!(res.params.len(), 13);
}

#[test]
fn test_subquery_catalog_validation() {
    let schema = r#"
        table orders [id integer primary auto, customer_id integer, total float]
        table customers [id integer primary auto, region string]
    "#;
    let catalog = pipeql_core::api::catalog_from_schema(schema).unwrap();

    // Valid columns in both tables
    let src = "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])";
    let res = pipeql_core::api::compile_with_catalog(src, "postgres", Some(&catalog)).unwrap();
    assert!(res.sql.contains("SELECT * FROM orders"));

    // Invalid column in subquery must be caught
    let invalid_sub = "from orders | filter customer_id in (from customers | filter non_existent == 'EU' | select [id])";
    let err = pipeql_core::api::compile_with_catalog(invalid_sub, "postgres", Some(&catalog)).unwrap_err();
    assert!(format!("{err}").contains("non_existent"));
}

#[test]
fn test_block_comments() {
    let src = "/* start comment */ from users /* inline comment */ | filter age > 18 /* end comment */";
    let res = pipeql_core::api::compile(src, "sqlite").unwrap();
    assert!(res.sql.contains("SELECT * FROM users"));
    assert!(res.sql.contains("WHERE (age > 18)"));
}

#[test]
fn test_native_catalog_from_schema_and_compile_with_schema() {
    let schema = "table users [id integer primary auto, name string, created_at timestamp]";
    let catalog = pipeql_core::api::catalog_from_schema(schema).unwrap();
    assert!(catalog.has_column("users", "created_at"));

    let res = pipeql_core::api::compile_with_schema("from users | filter id == $id", "sqlite", schema).unwrap();
    assert!(res.sql.contains("SELECT * FROM users"));
    assert_eq!(res.params, vec!["id"]);
}

