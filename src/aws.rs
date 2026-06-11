use aws_sdk_sustainability::primitives::DateTime;
use aws_sdk_sustainability::types::{Dimension, EmissionsType, TimePeriod, TimeGranularity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::period::YearMonth;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionsResult {
    pub month: String,
    pub region: String,
    pub service: String,
    pub lbm: f64,
    pub mbm: f64,
}

// ── AWS CLI JSON format ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CliOutput {
    results: Vec<CliResult>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CliResult {
    time_period: CliTimePeriod,
    dimensions_values: HashMap<String, String>,
    emissions_values: HashMap<String, CliEmissions>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CliTimePeriod {
    start: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CliEmissions {
    value: f64,
}

impl TryFrom<CliOutput> for Vec<EmissionsResult> {
    type Error = anyhow::Error;

    fn try_from(cli: CliOutput) -> anyhow::Result<Self> {
        cli.results
            .into_iter()
            .map(|r| {
                // "2025-01-01T00:00:00.000Z" → "2025-01"
                let month = r.time_period.start.get(..7)
                    .ok_or_else(|| anyhow::anyhow!("unexpected TimePeriod.Start format: {}", r.time_period.start))?
                    .to_string();
                let region = r.dimensions_values.get("REGION").cloned().unwrap_or_default();
                let service = r.dimensions_values.get("SERVICE").cloned().unwrap_or_default();
                let lbm = r.emissions_values.get("TOTAL_LBM_CARBON_EMISSIONS").map(|e| e.value).unwrap_or(0.0);
                let mbm = r.emissions_values.get("TOTAL_MBM_CARBON_EMISSIONS").map(|e| e.value).unwrap_or(0.0);
                Ok(EmissionsResult { month, region, service, lbm, mbm })
            })
            .collect()
    }
}

pub async fn get_estimated_carbon_emissions(
    profile: &str,
    from: YearMonth,
    to: YearMonth,
) -> anyhow::Result<Vec<EmissionsResult>> {
    if from > to {
        anyhow::bail!("--from ({from}) must not be after --to ({to})");
    }

    // Sustainability API is only available in us-east-1
    let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .profile_name(profile)
        .region(aws_config::Region::new("us-east-1"))
        .load()
        .await;

    let client = aws_sdk_sustainability::Client::new(&sdk_config);

    let start = DateTime::from_secs(from.start_timestamp());
    let end = DateTime::from_secs(to.end_timestamp());

    let time_period = TimePeriod::builder().start(start).end(end).build()?;

    let mut results = Vec::new();
    let mut next_token: Option<String> = None;

    loop {
        let mut req = client
            .get_estimated_carbon_emissions()
            .time_period(time_period.clone())
            .granularity(TimeGranularity::Monthly)
            .group_by(Dimension::Region)
            .group_by(Dimension::Service)
            .emissions_types(EmissionsType::TotalLbmCarbonEmissions)
            .emissions_types(EmissionsType::TotalMbmCarbonEmissions)
            .max_results(100);

        if let Some(token) = next_token {
            req = req.next_token(token);
        }

        let resp = req.send().await?;

        for item in resp.results() {
            let region = item
                .dimensions_values()
                .get(&Dimension::Region)
                .cloned()
                .unwrap_or_default();
            let service = item
                .dimensions_values()
                .get(&Dimension::Service)
                .cloned()
                .unwrap_or_default();

            // Time period start gives us the month: format as YYYY-MM
            let month = item
                .time_period()
                .map(|tp| {
                    let secs = tp.start().secs();
                    let naive = chrono::DateTime::from_timestamp(secs, 0)
                        .unwrap_or_default()
                        .naive_utc();
                    format!("{}-{:02}", naive.format("%Y"), naive.format("%m"))
                })
                .unwrap_or_default();

            let lbm = item
                .emissions_values()
                .get(&EmissionsType::TotalLbmCarbonEmissions)
                .map(|e| e.value())
                .unwrap_or(0.0);

            let mbm = item
                .emissions_values()
                .get(&EmissionsType::TotalMbmCarbonEmissions)
                .map(|e| e.value())
                .unwrap_or(0.0);

            results.push(EmissionsResult { month, region, service, lbm, mbm });
        }

        next_token = resp.next_token().map(str::to_owned);
        if next_token.is_none() {
            break;
        }
    }

    Ok(results)
}

#[cfg(test)]
pub fn mock_results(year: i32) -> Vec<EmissionsResult> {
    let months: Vec<String> = (1..=12).map(|m| format!("{year}-{m:02}")).collect();

    let profiles: &[(&str, &str, f64, [f64; 12])] = &[
        // eu-west-1
        ("AmazonEC2",         "eu-west-1",    0.218, [ 6.22,  8.47,  9.64,  9.32,  7.69, 10.62,  8.21,  6.78,  9.43, 11.07, 18.49, 10.18]),
        ("AmazonRDS",         "eu-west-1",    0.218, [ 7.64,  7.95,  8.21,  7.94,  7.69,  8.04,  7.82,  7.54,  8.18,  8.31,  8.82,  8.07]),
        ("AmazonSageMaker",   "eu-west-1",    0.218, [ 2.62,  3.47,  4.40,  5.49,  6.31,  6.82,  6.58,  5.87,  4.99,  3.97,  3.18,  2.40]),
        ("AmazonRedshift",    "eu-west-1",    0.218, [ 6.47,  4.40,  3.42,  4.18,  3.47,  3.38,  3.42,  3.29,  4.27,  5.02,  5.67,  6.31]),
        ("AmazonEKS",         "eu-west-1",    0.218, [ 2.82,  3.18,  3.60,  3.93,  4.07,  4.47,  4.40,  4.07,  4.18,  4.53,  4.47,  3.27]),
        ("AWSGlue",           "eu-west-1",    0.218, [ 4.82,  3.65,  3.02,  4.47,  3.29,  3.07,  3.42,  3.78,  4.18,  4.91,  4.47,  3.87]),
        ("AmazonElastiCache", "eu-west-1",    0.218, [ 2.18,  2.25,  2.31,  2.27,  2.22,  2.38,  2.36,  2.20,  2.33,  2.42,  2.58,  2.27]),
        ("AmazonDynamoDB",    "eu-west-1",    0.218, [ 1.51,  1.54,  1.58,  1.55,  1.52,  1.60,  1.58,  1.50,  1.58,  1.62,  1.76,  1.60]),
        ("AmazonS3",          "eu-west-1",    0.218, [ 0.23,  0.21,  0.24,  0.23,  0.25,  0.25,  0.28,  0.29,  0.29,  0.30,  0.33,  0.38]),
        ("AWSLambda",         "eu-west-1",    0.218, [ 0.17,  0.18,  0.19,  0.18,  0.20,  0.21,  0.20,  0.19,  0.20,  0.22,  0.23,  0.22]),
        // eu-central-1
        ("AmazonEC2",         "eu-central-1", 0.218, [ 2.38,  3.22,  3.67,  3.55,  2.93,  4.05,  3.12,  2.58,  3.59,  4.22,  7.04,  3.88]),
        ("AmazonRDS",         "eu-central-1", 0.218, [ 1.65,  1.71,  1.76,  1.71,  1.66,  1.73,  1.69,  1.62,  1.73,  1.76,  1.87,  1.73]),
        ("AmazonS3",          "eu-central-1", 0.218, [ 0.09,  0.08,  0.08,  0.08,  0.08,  0.08,  0.06,  0.06,  0.05,  0.04,  0.05,  0.05]),
        // us-east-1
        ("AmazonEC2",         "us-east-1",    0.218, [ 1.07,  1.45,  1.65,  1.59,  1.31,  1.80,  1.40,  1.15,  1.60,  1.88,  3.14,  1.73]),
        ("AmazonCloudFront",  "us-east-1",    0.218, [ 0.16,  0.18,  0.20,  0.21,  0.22,  0.24,  0.24,  0.24,  0.22,  0.20,  0.19,  0.18]),
        ("AmazonS3",          "us-east-1",    0.218, [ 0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04,  0.04]),
        ("AWSLambda",         "us-east-1",    0.218, [ 0.08,  0.09,  0.10,  0.09,  0.10,  0.10,  0.10,  0.10,  0.10,  0.11,  0.12,  0.11]),
        // eu-west-2
        ("AmazonEC2",         "eu-west-2",    0.218, [ 0.76,  1.02,  1.16,  1.12,  0.92,  1.28,  0.99,  0.81,  1.13,  1.33,  2.22,  1.22]),
        ("AmazonRDS",         "eu-west-2",    0.218, [ 0.62,  0.65,  0.67,  0.65,  0.63,  0.66,  0.64,  0.61,  0.65,  0.67,  0.71,  0.65]),
    ];

    profiles
        .iter()
        .flat_map(|(service, region, mbm_ratio, monthly)| {
            months.iter().zip(monthly.iter()).map(move |(month, &lbm)| EmissionsResult {
                month: month.clone(),
                region: region.to_string(),
                service: service.to_string(),
                lbm,
                mbm: lbm * mbm_ratio,
            })
        })
        .collect()
}
