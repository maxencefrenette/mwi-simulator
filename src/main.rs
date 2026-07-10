use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use mwi_simulator::{
    data::{OFFICIAL_MARKETPLACE_URL, fetch_official_marketplace_to_path, read_market_snapshot},
    history::{fetch_all_market_history, validate_history_request},
    player::PlayerExport,
    rank_actions::{RankActionsConfig, rank_actions},
    recommend_orders::{RecommendOrdersConfig, recommend_orders},
    wealth::calculate_wealth,
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
    /// Fetch base-item third-party market history for every item in a market snapshot.
    FetchAllHistory {
        #[arg(long)]
        market: PathBuf,
        #[arg(long)]
        output_dir: PathBuf,
        /// Number of days requested from the history source.
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// Delay between network attempts.
        #[arg(long, default_value_t = 1000)]
        delay_ms: u64,
        /// Ignore the seven-day cache freshness guard.
        #[arg(long)]
        force: bool,
    },
    /// Calculate pessimistic player wealth from a CDP player export and market bids.
    Wealth {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
    },
    /// Rank unlocked noncombat actions with history-aware sell-through adjustment.
    RankActions {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long)]
        history_dir: PathBuf,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    /// Recommend persistent buy orders that unlock valuable 24h action packages.
    RecommendOrders {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long)]
        history_dir: PathBuf,
        #[arg(long, default_value_t = 10)]
        max_orders: usize,
        #[arg(long, default_value_t = 5)]
        alternatives: usize,
        #[arg(long, default_value_t = 1.0)]
        tick_size: f64,
        #[arg(long, default_value_t = 0.05)]
        volume_participation_rate: f64,
        #[arg(long, default_value_t = 0.05)]
        daily_discount_rate: f64,
        #[arg(long, default_value_t = 0.001)]
        daily_capital_cost_rate: f64,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::FetchMarket { output } => {
            fetch_official_marketplace_to_path(&output)?;
            eprintln!("Fetched {OFFICIAL_MARKETPLACE_URL} to {}", output.display());
        }
        Command::FetchAllHistory {
            market,
            output_dir,
            days,
            delay_ms,
            force,
        } => {
            validate_history_request(days)?;
            let market = read_market_snapshot(&market)?;
            let report = fetch_all_market_history(
                &market,
                &output_dir,
                days,
                Duration::from_millis(delay_ms),
                force,
            );

            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.failed > 0 {
                anyhow::bail!("failed to fetch {} history files", report.failed);
            }
        }
        Command::Wealth { player, market } => {
            let player = read_json::<PlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let wealth = calculate_wealth(&player, &market);

            println!("{}", serde_json::to_string_pretty(&wealth)?);
        }
        Command::RankActions {
            player,
            market,
            history_dir,
            limit,
        } => {
            let player = read_json::<PlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let actions =
                rank_actions(&player, &market, &history_dir, RankActionsConfig { limit })?;

            println!("{}", serde_json::to_string_pretty(&actions)?);
        }
        Command::RecommendOrders {
            player,
            market,
            history_dir,
            max_orders,
            alternatives,
            tick_size,
            volume_participation_rate,
            daily_discount_rate,
            daily_capital_cost_rate,
        } => {
            let player = read_json::<PlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let recommendation = recommend_orders(
                &player,
                &market,
                &history_dir,
                RecommendOrdersConfig {
                    max_orders,
                    alternatives,
                    tick_size,
                    volume_participation_rate,
                    daily_discount_rate,
                    daily_capital_cost_rate,
                },
            )?;

            println!("{}", serde_json::to_string_pretty(&recommendation)?);
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
