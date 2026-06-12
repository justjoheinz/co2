# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```sh
cargo build --release        # production binary → target/release/co2
cargo build                  # debug build
cargo test                   # all tests
cargo test <name>            # single test by name (substring match)
cargo test -- --nocapture    # show println! output during tests
cargo clippy                 # lint
cargo run -- --data resources/emissions_2024_2025.json  # run from sample file
cargo install --path . --locked  # install binary; --locked required to avoid coherence errors from fresh dep resolution
```

## Architecture

The app has two execution paths that converge on a shared `Vec<EmissionsResult>`:

1. **Live query** (`aws.rs`: `get_estimated_carbon_emissions`) — calls the AWS Sustainability SDK, paginates results, normalises them into `EmissionsResult`.
2. **File mode** (`aws.rs`: `parse_emissions_json`) — deserialises the raw JSON output of `aws sustainability get-estimated-carbon-emissions` (PascalCase schema via `CliOutput`) into the same `EmissionsResult` type.

`main.rs` drives CLI parsing (clap derive), selects the path, constructs a title string, then hands off to the ratatui TUI.

### Modules

| Module | Role |
|--------|------|
| `aws` | Data model (`EmissionsResult`), AWS SDK call, JSON parser, `mock_results` for tests |
| `period` | `YearMonth` / `YearMonthEnd` — CLI arg parsing and Unix timestamp helpers |
| `summary` | Pure aggregation helpers: `sum_by`, `top_n`, formatting (`fmt_co2`, `fmt_cell`, `fmt_pct`), `year_range_title` |
| `display` | All ratatui rendering: `AppState` (scroll), `render` function, individual widget builders |

### Data flow

```
CLI args → Cli struct
             ↓
        aws::get_estimated_carbon_emissions   (live)
        aws::parse_emissions_json             (file)
             ↓
        Vec<EmissionsResult>  {month, region, service, lbm, mbm}
             ↓
        summary::{sum_by, top_n}  →  display::render  →  ratatui Terminal
```

`EmissionsResult` stores two emission metrics: `lbm` (location-based) and `mbm` (market-based), both in MTCO2e. `fmt_co2` formats values without the unit suffix; the unit label appears in table headers only.

### TUI

`display::render` is called on every frame. It builds all widgets from scratch each frame — no retained widget state beyond `AppState` (scroll offset). The chart uses ratatui's native `Chart`/`Dataset` (not an external crate). The heatmap cell width is computed dynamically via `fmt_cell`, which degrades precision to fit.

### Sample data

`resources/*.json` contain real-shaped AWS API responses usable with `--data` for local development without AWS credentials.
