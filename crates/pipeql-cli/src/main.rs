use std::fs;

use clap::{Args, Parser as ClapParser, Subcommand};
use pipeql_core::PipeQLError;
use serde::Serialize;

#[derive(ClapParser)]
#[command(name = "pipeql")]
#[command(about = "PipeQL - pipelined & injection-safe polyglot query language compiler")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a PipeQL query into target-dialect SQL with extracted parameters.
    Compile(CompileArgs),
    /// Parse a PipeQL query into a JSON AST (lossless, with spans).
    Parse(ParseArgs),
    /// List supported SQL dialects.
    SupportedDialects,
}

#[derive(Args)]
struct CompileArgs {
    /// The PipeQL query source (use quotes for multi-line input).
    query: String,
    /// Target dialect: postgres (default), sqlite, duckdb, mysql.
    #[arg(long, short, default_value = "postgres")]
    dialect: String,
    /// Path to a JSON catalog file for column validation.
    #[arg(long)]
    catalog: Option<String>,
    /// Path to a PipeQL schema file (.pql) or raw table DDL string for column validation.
    #[arg(long)]
    schema: Option<String>,
    /// Emit machine-readable JSON.
    #[arg(long)]
    json: bool,
    /// Suppress printing extracted parameter names to stderr (default when not --json).
    #[arg(long)]
    no_params: bool,
}

#[derive(Args)]
struct ParseArgs {
    /// The PipeQL query source (use quotes for multi-line input).
    query: String,
}

#[derive(Serialize)]
struct JsonOutput {
    sql: String,
    params: Vec<String>,
    dialect: String,
    statement_type: String,
    is_mutation: bool,
    parameter_count: usize,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Compile(args) => run_compile(args),
        Commands::Parse(args) => run_parse(args),
        Commands::SupportedDialects => run_supported_dialects(),
    }
}

fn run_compile(args: CompileArgs) {
    let result = if let Some(schema_arg) = &args.schema {
        let schema_str = match fs::read_to_string(schema_arg) {
            Ok(content) => content,
            Err(_) => schema_arg.clone(),
        };
        pipeql_core::api::compile_with_schema(&args.query, &args.dialect, &schema_str)
    } else if let Some(catalog_path) = &args.catalog {
        let catalog_json = match fs::read_to_string(catalog_path) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Error reading catalog file: {e}");
                std::process::exit(1);
            }
        };
        let catalog: pipeql_core::Catalog = match serde_json::from_str(&catalog_json) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error parsing catalog JSON: {e}");
                std::process::exit(1);
            }
        };
        pipeql_core::api::compile_with_catalog(&args.query, &args.dialect, Some(&catalog))
    } else {
        pipeql_core::api::compile(&args.query, &args.dialect)
    };

    match result {
        Ok(compiled) => {
            if args.json {
                let out = JsonOutput {
                    sql: compiled.sql,
                    params: compiled.params.clone(),
                    dialect: args.dialect,
                    statement_type: compiled.statement_type.as_str().to_string(),
                    is_mutation: compiled.is_mutation,
                    parameter_count: compiled.params.len(),
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&out).expect("serializing output must not fail")
                );
            } else {
                println!("{}", compiled.sql);
                if !args.no_params && !compiled.params.is_empty() {
                    eprintln!("Parameters: {:?}", compiled.params);
                }
            }
        }
        Err(err) => {
            render_error(&err);
            std::process::exit(1);
        }
    }
}

fn run_parse(args: ParseArgs) {
    match pipeql_core::api::parse_statement(&args.query) {
        Ok(stmt) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&stmt).expect("serializing AST must not fail")
            );
        }
        Err(err) => {
            render_error(&err);
            std::process::exit(1);
        }
    }
}

fn run_supported_dialects() {
    let dialects = pipeql_core::api::supported_dialects();
    for d in dialects {
        println!("{d}");
    }
}

fn render_error(err: &PipeQLError) {
    match err {
        PipeQLError::Parse(errs) => {
            for e in errs {
                eprintln!("{}", e.message);
                if let Some(s) = &e.suggestion {
                    eprintln!("  hint: {s}");
                }
            }
        }
        PipeQLError::Analysis(errs) => {
            for e in errs {
                eprintln!("{}", e.message);
                if let Some(s) = &e.suggestion {
                    eprintln!("  hint: {s}");
                }
            }
        }
        PipeQLError::Codegen(e) => eprintln!("{e}"),
    }
}
