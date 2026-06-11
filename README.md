# co2

Interactive terminal UI for AWS Carbon Footprint data.

Queries the [AWS Sustainability API](https://docs.aws.amazon.com/aws-cost-management/latest/APIReference/API_sustainability_GetEstimatedCarbonEmissions.html) and displays monthly emissions broken down by region and service, with a stacked bar chart, ranked tables, and a service × month heatmap.

## Usage

```
co2 --profile <profile> --from <YYYY[-MM]> [--to <YYYY[-MM]>]
co2 --data <file.json>
```

### Live query

```sh
# Full year
co2 --profile myprofile --from 2024

# Date range
co2 --profile myprofile --from 2024-06 --to 2025-03
```

`--from` is required. `--to` defaults to the current month. Both accept `YYYY` (expands to Jan/Dec respectively) or `YYYY-MM`.

The Sustainability API is only available in `us-east-1`; the tool targets that region automatically.

### From file

```sh
co2 --data results.json
```

Accepts the raw JSON output of:

```sh
aws sustainability get-estimated-carbon-emissions \
  --region us-east-1 \
  --granularity MONTHLY \
  --group-by REGION SERVICE \
  --time-period Start=YYYY-MM-DD,End=YYYY-MM-DD
```

## Keybindings

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `↑` / `k` | Scroll up |
| `↓` / `j` | Scroll down |
| `PgUp` / `PgDn` | Scroll by 10 |

## Building

```sh
cargo build --release
```

Requires an AWS profile with permissions for `sustainability:GetEstimatedCarbonEmissions`.
