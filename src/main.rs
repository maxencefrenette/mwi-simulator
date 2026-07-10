use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, Subcommand};
use mwi_simulator::{
    data::{OFFICIAL_MARKETPLACE_URL, fetch_official_marketplace_to_path, read_market_snapshot},
    history::{fetch_all_market_history, validate_history_request},
    plan::{PlanConfig, plan},
    player::PlayerExport,
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
    /// Refresh the official market snapshot and stale historical market data.
    FetchMarket {
        #[arg(long, default_value = ".local/market.current.json")]
        output: PathBuf,
        #[arg(long, default_value = ".local/market-history")]
        history_dir: PathBuf,
        /// Number of days requested from the history source.
        #[arg(long, default_value_t = 30)]
        days: u32,
        /// Delay between network attempts.
        #[arg(long, default_value_t = 1000)]
        delay_ms: u64,
        /// Ignore the seven-day cache freshness guard.
        #[arg(long)]
        force_history: bool,
    },
    /// Calculate pessimistic player wealth from a CDP player export and market bids.
    Wealth {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
    },
    /// Rank actions and recommend persistent market orders.
    Plan {
        #[arg(long)]
        player: PathBuf,
        #[arg(long)]
        market: PathBuf,
        #[arg(long)]
        history_dir: PathBuf,
        #[arg(long, default_value_t = 25)]
        action_limit: usize,
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
        Command::FetchMarket {
            output,
            history_dir,
            days,
            delay_ms,
            force_history,
        } => {
            validate_history_request(days)?;
            fetch_official_marketplace_to_path(&output)?;
            eprintln!("Fetched {OFFICIAL_MARKETPLACE_URL} to {}", output.display());

            let market = read_market_snapshot(&output)?;
            let report = fetch_all_market_history(
                &market,
                &history_dir,
                days,
                Duration::from_millis(delay_ms),
                force_history,
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
        Command::Plan {
            player,
            market,
            history_dir,
            action_limit,
            max_orders,
            alternatives,
            tick_size,
            volume_participation_rate,
            daily_discount_rate,
            daily_capital_cost_rate,
        } => {
            let player = read_json::<PlayerExport>(&player)?;
            let market = read_market_snapshot(&market)?;
            let daily_plan = plan(
                &player,
                &market,
                &history_dir,
                PlanConfig {
                    action_limit,
                    max_orders,
                    alternatives,
                    tick_size,
                    volume_participation_rate,
                    daily_discount_rate,
                    daily_capital_cost_rate,
                },
            )?;

            println!("{}", serde_json::to_string_pretty(&daily_plan)?);
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
