use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;
use spg_core::{
    build_asset_index, default_asset_path, load_asset_bundle, load_raw_items_from_jsonl,
    sample_input_path, SPGInputPipeline, SQLiteStorage,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(author, version, about = "SPG input layer MVP in Rust")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    InitDb {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        assets: Option<PathBuf>,
    },
    Demo {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        assets: Option<PathBuf>,
        #[arg(long)]
        input: Option<PathBuf>,
    },
    Ingest {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        assets: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::InitDb { db, assets } => {
            init_db(resolve_db_path(db), resolve_assets_path(assets))
        }
        Commands::Demo { db, assets, input } => {
            let input = input.unwrap_or_else(sample_input_path);
            ingest(&input, &resolve_db_path(db), &resolve_assets_path(assets))
        }
        Commands::Ingest { input, db, assets } => {
            ingest(&input, &resolve_db_path(db), &resolve_assets_path(assets))
        }
    }
}

fn init_db(db_path: PathBuf, asset_path: PathBuf) -> Result<()> {
    let bundle = load_asset_bundle(&asset_path)?;
    let storage = SQLiteStorage::new(&db_path);
    storage.initialize()?;
    storage.seed_aliases(&bundle)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "initialized",
            "db": db_path,
            "assets": asset_path,
            "row_counts": storage.row_counts()?,
        }))?
    );
    Ok(())
}

fn ingest(input_path: &Path, db_path: &Path, asset_path: &Path) -> Result<()> {
    let bundle = load_asset_bundle(asset_path)?;
    let asset_index = build_asset_index(bundle.clone());
    let items = load_raw_items_from_jsonl(input_path)?;
    let pipeline = SPGInputPipeline::new(asset_index);
    let result = pipeline.process(items);

    let storage = SQLiteStorage::new(db_path);
    storage.initialize()?;
    storage.seed_aliases(&bundle)?;
    storage.persist_result(&result)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "ok",
            "run_id": result.run_id,
            "asset_version": result.asset_version,
            "rule_version": result.rule_version,
            "db": db_path,
            "input": input_path,
            "metrics": result.metrics,
            "row_counts": storage.row_counts()?,
            "duplicate_groups": result.dedupe_groups,
            "unresolved_records": result.unresolved_records,
        }))?
    );

    Ok(())
}

fn resolve_db_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("data")
            .join("spg_input_layer.db")
    })
}

fn resolve_assets_path(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(default_asset_path)
}
