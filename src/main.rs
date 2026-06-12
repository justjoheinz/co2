mod aws;
mod display;
mod period;
mod summary;

use period::{YearMonth, YearMonthEnd};

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{io, path::PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "co2",
    version = env!("CARGO_PKG_VERSION"),
    about = "AWS Carbon Footprint Reporter — interactive TUI for CO2 emissions data",
    long_about = "\
AWS Carbon Footprint Reporter

Queries the AWS Sustainability API and displays an interactive terminal report
with monthly breakdowns, top regions, top services, and a service × month
heatmap. Press 'q' or Esc to exit.

LIVE QUERY
  co2 --profile myprofile --from 2024
  co2 --profile myprofile --from 2024-06 --to 2025-03

  --from is required. --to is optional and defaults to the current month.
  Both accept YYYY (Jan for --from, Dec for --to) or YYYY-MM.

FROM FILE
  co2 --data results.json

  Accepts the raw JSON output of:
    aws sustainability get-estimated-carbon-emissions \\
      --region us-east-1 --granularity MONTHLY \\
      --group-by REGION SERVICE \\
      --time-period Start=YYYY-MM-DD,End=YYYY-MM-DD

  --profile, --from, and --to are forbidden when --data is used.

UNITS
  All emissions values are in MTCO2e (Metric Tons of CO2 equivalent).
",
)]
struct Cli {
    /// AWS profile name (from ~/.aws/config); falls back to $AWS_PROFILE
    #[arg(short, long, conflicts_with = "data")]
    profile: Option<String>,

    /// Start of query range: YYYY or YYYY-MM (required)
    #[arg(long, value_name = "YYYY[-MM]", conflicts_with = "data", required_unless_present = "data")]
    from: Option<YearMonth>,

    /// End of query range: YYYY or YYYY-MM, inclusive; YYYY expands to Dec of that year (default: current month)
    #[arg(long, value_name = "YYYY[-MM]", conflicts_with = "data")]
    to: Option<YearMonthEnd>,

    /// Read emissions JSON from a file instead of querying AWS
    #[arg(long, value_name = "FILE", conflicts_with = "from", conflicts_with = "to", conflicts_with = "profile")]
    data: Option<PathBuf>,

    /// Override the title displayed in the TUI
    #[arg(long, value_name = "TEXT")]
    title: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let (results, title) = if let Some(path) = &cli.data {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", path.display(), e))?;
        let results = aws::parse_emissions_json(&contents)?;
        let title = summary::year_range_title(&results, path.to_string_lossy().as_ref());
        (results, title)
    } else {
        let profile = cli.profile
            .or_else(|| std::env::var("AWS_PROFILE").ok())
            .ok_or_else(|| anyhow::anyhow!("no AWS profile: set --profile or $AWS_PROFILE"))?;
        let from = cli.from.expect("--from is required without --data");
        let to = cli.to.map(|e| e.0).unwrap_or_else(YearMonth::current);
        let results = aws::get_estimated_carbon_emissions(&profile, from, to).await?;
        let title = if from.year == to.year && from.month == 1 && to.month == 12 {
            format!("{} — {}", profile, from.year)
        } else {
            format!("{} — {from}–{to}", profile)
        };
        (results, title)
    };

    let title = cli.title.unwrap_or(title);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = display::AppState::new();
    let result = run_loop(&mut terminal, &results, &title, &mut state);

    // Always restore terminal, even if the loop errored
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    result
}

fn run_loop(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
    results: &[aws::EmissionsResult],
    title: &str,
    state: &mut display::AppState,
) -> anyhow::Result<()> {
    loop {
        terminal.draw(|f| display::render(f, results, title, state))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Up   | KeyCode::Char('k') => state.scroll_up(1),
                KeyCode::Down | KeyCode::Char('j') => state.scroll_down(1),
                KeyCode::PageUp                    => state.scroll_up(10),
                KeyCode::PageDown                  => state.scroll_down(10),
                _ => {}
            }
        }
    }
    Ok(())
}
