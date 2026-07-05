use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use mwi_simulator::{
    data::{
        fetch_official_marketplace_to_path, read_market_snapshot, summarize_market_snapshot,
        OFFICIAL_MARKETPLACE_URL,
    },
    money_actions::{best_money_actions, ActionPlayerExport},
    recommend_sells,
    valuation::conservative_terminal_wealth,
    wealth::{calculate_wealth, PlayerExport},
    PlayerState, ProductionPlan, SellRecommendationConfig, ValuationConfig,
};

#[derive(Debug, Parser)]
#[command(version, about = "Milky Way Idle daily wealth optimizer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Fetch the official MWI marketplace snapshot to a JSON file.
    FetchMarket {
        #[arg(long)]
        output: PathBuf,
    },
    /// Summarize a normalized or official raw market snapshot.
    SummarizeMarket {
        #[arg(long)]
        market: PathBuf,
    },
    /// Calculate pessimistic player wealth from a CDP player export and market bids.
    Wealth {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
    },
    /// Rank unlocked noncombat actions by approximate market profit per hour.
    MoneyActions {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Recommend sell-side orders for current inventory plus expected 24h production.
    RecommendSells {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long)]
        production: Option<PathBuf>,
        #[arg(long, default_value_t = 10)]
        max_orders: usize,
        #[arg(long, default_value_t = 1.0)]
        tick_size: f64,
        #[arg(long, default_value_t = 0.15)]
        liquidity_haircut: f64,
    },
    /// Compute conservative terminal wealth for the current state.
    Value {
        #[arg(long)]
        state: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long, default_value_t = 0.15)]
        liquidity_haircut: f64,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::FetchMarket { output } => {
            fetch_official_marketplace_to_path(&output)?;
            eprintln!("Fetched {OFFICIAL_MARKETPLACE_URL} to {}", output.display());
        }
        Command::SummarizeMarket { market } => {
            let market = read_market_snapshot(&market)?;
            let summary = summarize_market_snapshot(&market);

            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        Command::Wealth { player, market } => {
            let player = read_json::<PlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let wealth = calculate_wealth(&player, &market);

            println!("{}", serde_json::to_string_pretty(&wealth)?);
        }
        Command::MoneyActions {
            player,
            market,
            limit,
        } => {
            let player = read_json::<ActionPlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let actions = best_money_actions(&player, &market, limit);

            println!("{}", serde_json::to_string_pretty(&actions)?);
        }
        Command::RecommendSells {
            state,
            market,
            production,
            max_orders,
            tick_size,
            liquidity_haircut,
        } => {
            let state = read_json::<PlayerState>(&state)?;
            let market = read_market_snapshot(&market)?;
            let production = match production {
                Some(path) => read_json::<ProductionPlan>(&path)?,
                None => ProductionPlan::empty(),
            };

            let recommendations = recommend_sells(
                &state,
                &market,
                &production,
                SellRecommendationConfig {
                    max_orders,
                    tick_size,
                    valuation: ValuationConfig { liquidity_haircut },
                },
            );

            println!("{}", serde_json::to_string_pretty(&recommendations)?);
        }
        Command::Value {
            state,
            market,
            liquidity_haircut,
        } => {
            let state = read_json::<PlayerState>(&state)?;
            let market = read_market_snapshot(&market)?;
            let valuation = conservative_terminal_wealth(
                &state,
                &market,
                ValuationConfig { liquidity_haircut },
            );

            println!("{}", serde_json::to_string_pretty(&valuation)?);
        }
    }

    Ok(())
}

fn read_json<T>(path: &PathBuf) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::from_reader(file).with_context(|| format!("failed to parse {}", path.display()))
}
