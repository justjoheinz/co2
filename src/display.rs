use std::collections::{BTreeMap, HashSet};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Widget},
    Frame,
};
use ratatui_stacked_bar::StackedSparkline;

use crate::aws::EmissionsResult;

// ── Aggregation helpers ──────────────────────────────────────────────────────

// Returns a BTreeMap so iteration is always in sorted key order.
fn sum_by<K, F, G>(results: &[EmissionsResult], key: F, val: G) -> BTreeMap<K, f64>
where
    K: Ord,
    F: Fn(&EmissionsResult) -> K,
    G: Fn(&EmissionsResult) -> f64,
{
    let mut map: BTreeMap<K, f64> = BTreeMap::new();
    for r in results {
        *map.entry(key(r)).or_default() += val(r);
    }
    map
}

fn top_n(map: &BTreeMap<String, f64>, n: usize) -> Vec<(String, f64)> {
    let mut v: Vec<_> = map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    v.truncate(n);
    v
}

// ── Formatting ───────────────────────────────────────────────────────────────

fn fmt_co2(v: f64) -> String {
    format!("{:.2} MTCO2e", v)
}

fn fmt_cell(v: f64, width: usize) -> String {
    if v == 0.0 {
        return String::new();
    }
    // Try progressively shorter representations until one fits
    for s in [
        format!("{:.2}", v),
        format!("{:.1}", v),
        format!("{:.0}", v),
    ] {
        if s.len() <= width {
            return s;
        }
    }
    String::new()
}

fn fmt_pct(v: f64, total: f64) -> String {
    if total == 0.0 {
        "  0.0%".to_string()
    } else {
        format!("{:.1}%", v / total * 100.0)
    }
}

// ── Heatmap widget ───────────────────────────────────────────────────────────

pub struct Heatmap<'a> {
    /// row label, then one value per column
    rows: Vec<(String, Vec<f64>)>,
    col_labels: Vec<String>,
    title: &'a str,
}

impl<'a> Heatmap<'a> {
    pub fn new(title: &'a str, rows: Vec<(String, Vec<f64>)>, col_labels: Vec<String>) -> Self {
        Self { rows, col_labels, title }
    }
}

impl Widget for Heatmap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::default().title(self.title).borders(Borders::ALL);
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 2 || inner.width < 4 {
            return;
        }

        let max = self
            .rows
            .iter()
            .flat_map(|(_, vals)| vals.iter().copied())
            .fold(0.0_f64, f64::max);

        // Column width: distribute inner width evenly across label + cols
        let n_cols = self.col_labels.len();
        if n_cols == 0 {
            return;
        }
        let label_w = self.rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0).max(8) as u16;
        let cell_w = ((inner.width.saturating_sub(label_w + 1)) / n_cols as u16).max(1);

        // Header row (col labels)
        let mut x = inner.x + label_w + 1;
        for label in &self.col_labels {
            let truncated: String = label.chars().take(cell_w as usize).collect();
            buf.set_string(x, inner.y, &truncated, Style::default().bold());
            x += cell_w;
        }

        // Data rows
        for (row_idx, (row_label, values)) in self.rows.iter().enumerate() {
            let y = inner.y + 1 + row_idx as u16;
            if y >= inner.y + inner.height {
                break;
            }

            // Row label
            let truncated: String = row_label.chars().take(label_w as usize).collect();
            buf.set_string(inner.x, y, &truncated, Style::default());

            // Cells
            let mut x = inner.x + label_w + 1;
            for &val in values {
                let intensity = if max > 0.0 { val / max } else { 0.0 };
                let bg = heat_color(intensity);
                // Choose fg so it contrasts with the background brightness
                let fg = if intensity > 0.55 { Color::Black } else { Color::White };
                let label = fmt_cell(val, cell_w as usize);
                // Pad to exactly cell_w with spaces
                let padded = format!("{:width$}", label, width = cell_w as usize);
                buf.set_string(x, y, &padded, Style::default().fg(fg).bg(bg));
                x += cell_w;
            }
        }
    }
}

// ── Stretched stacked sparkline ──────────────────────────────────────────────

/// Wraps `StackedSparkline` and repeats each data point so bars fill the full
/// width of the area, regardless of how many data points there are.
struct StretchedSparkline {
    series: Vec<(Vec<usize>, Color)>,
    max: usize,
}

impl StretchedSparkline {
    fn new(series: Vec<(Vec<usize>, Color)>, max: usize) -> Self {
        Self { series, max }
    }
}

impl Widget for StretchedSparkline {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let n = self.series.first().map(|(v, _)| v.len()).unwrap_or(1).max(1);
        let repeat = (area.width as usize / n).max(1);

        let stretched: Vec<(Vec<usize>, Color)> = self
            .series
            .into_iter()
            .map(|(vals, color)| {
                let v = vals.iter().flat_map(|&x| std::iter::repeat(x).take(repeat)).collect();
                (v, color)
            })
            .collect();

        let mut sparkline = StackedSparkline::default().max(self.max);
        for (v, c) in stretched {
            sparkline = sparkline.add_data(v, c);
        }
        sparkline.render(area, buf);
    }
}

fn heat_color(t: f64) -> Color {
    // Multi-stop gradient: black → blue → cyan → green → yellow → orange → red
    let stops: &[(f64, (u8, u8, u8))] = &[
        (0.00, (  0,   0,   0)),
        (0.15, (  0,   0, 200)),
        (0.35, (  0, 210, 210)),
        (0.55, (  0, 200,   0)),
        (0.70, (230, 230,   0)),
        (0.85, (255, 140,   0)),
        (1.00, (230,   0,   0)),
    ];

    let t = t.clamp(0.0, 1.0);

    // Find the two stops that bracket t
    let i = stops.partition_point(|&(s, _)| s <= t).saturating_sub(1);
    let i = i.min(stops.len() - 2);
    let (t0, (r0, g0, b0)) = stops[i];
    let (t1, (r1, g1, b1)) = stops[i + 1];

    let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
    let lerp = |a: u8, b: u8| (a as f64 + f * (b as f64 - a as f64)) as u8;

    Color::Rgb(lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

// ── Main render entry point ───────────────────────────────────────────────────

pub struct AppState {
    pub monthly_table: TableState,
    pub monthly_len: usize,
}

impl AppState {
    pub fn new() -> Self {
        let mut monthly_table = TableState::default();
        monthly_table.select(Some(0));
        Self { monthly_table, monthly_len: 0 }
    }

    pub fn scroll_up(&mut self, n: usize) {
        let i = self.monthly_table.selected().unwrap_or(0).saturating_sub(n);
        self.monthly_table.select(Some(i));
    }

    pub fn scroll_down(&mut self, n: usize) {
        let i = (self.monthly_table.selected().unwrap_or(0) + n)
            .min(self.monthly_len.saturating_sub(1));
        self.monthly_table.select(Some(i));
    }
}

pub fn render(f: &mut Frame, results: &[EmissionsResult], title: &str, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),  // header (outer border 2 + inner border 2 + label + value + gap)
            Constraint::Length(16), // monthly chart + table (border 2 + header 1 + 12 months + 1)
            Constraint::Length(14), // regions + services
            Constraint::Min(10),    // heatmap
        ])
        .split(f.area());

    render_header(f, chunks[0], results, title);
    render_monthly(f, chunks[1], results, state);
    render_region_service(f, chunks[2], results);
    render_heatmap(f, chunks[3], results);
}

// ── Header ───────────────────────────────────────────────────────────────────

fn render_header(f: &mut Frame, area: Rect, results: &[EmissionsResult], title: &str) {
    let total_lbm: f64 = results.iter().map(|r| r.lbm).sum();
    let total_mbm: f64 = results.iter().map(|r| r.mbm).sum();
    let n_regions = results.iter().map(|r| r.region.as_str()).collect::<HashSet<_>>().len();

    let block = Block::default()
        .title(format!(" CO2 Emissions — {title} "))
        .borders(Borders::ALL);
    let inner = block.inner(area);
    block.render(area, f.buffer_mut());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 4); 4])
        .split(inner);

    for (i, (label, value)) in [
        ("Grand Total (LBM)", fmt_co2(total_lbm)),
        ("Grand Total (MBM)", fmt_co2(total_mbm)),
        ("Records",           results.len().to_string()),
        ("Regions",           n_regions.to_string()),
    ]
    .iter()
    .enumerate()
    {
        let text = vec![
            Line::from(Span::styled(*label, Style::default().bold())),
            Line::from(value.as_str()),
        ];
        f.render_widget(
            Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
            cols[i],
        );
    }
}

// ── Monthly breakdown ────────────────────────────────────────────────────────

fn render_monthly(f: &mut Frame, area: Rect, results: &[EmissionsResult], state: &mut AppState) {
    let lbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.lbm);
    let mbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.mbm);
    let months: Vec<String> = lbm_by_month.keys().cloned().collect();

    state.monthly_len = months.len();

    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(area);

    // Stacked sparkline (LBM bottom, MBM on top)
    let scale = 1000.0;
    let lbm_series: Vec<usize> = months.iter().map(|m| (*lbm_by_month.get(m).unwrap_or(&0.0) * scale) as usize).collect();
    let mbm_series: Vec<usize> = months.iter().map(|m| (*mbm_by_month.get(m).unwrap_or(&0.0) * scale) as usize).collect();
    let chart_max = lbm_series.iter().zip(mbm_series.iter()).map(|(l, m)| l + m).max().unwrap_or(1);

    let chart_block = Block::default().title(" Monthly LBM + MBM ").borders(Borders::ALL);
    let chart_inner = chart_block.inner(halves[0]);
    chart_block.render(halves[0], f.buffer_mut());
    StretchedSparkline::new(
        vec![(lbm_series, Color::Cyan), (mbm_series, Color::Blue)],
        chart_max,
    )
    .render(chart_inner, f.buffer_mut());

    // Scrollable table
    let rows: Vec<Row> = months
        .iter()
        .map(|m| {
            let lbm = lbm_by_month.get(m).copied().unwrap_or(0.0);
            let mbm = mbm_by_month.get(m).copied().unwrap_or(0.0);
            Row::new(vec![
                Cell::from(m.as_str()),
                Cell::from(fmt_co2(lbm)),
                Cell::from(fmt_co2(mbm)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Length(8), Constraint::Length(16), Constraint::Length(16)],
    )
    .header(Row::new(["Month", "LBM", "MBM"]).bold())
    .row_highlight_style(Style::default().bold())
    .block(Block::default().title(" Monthly Values (↑↓ to scroll) ").borders(Borders::ALL));

    let table_area = halves[1];
    f.render_stateful_widget(table, table_area, &mut state.monthly_table);

    // Scrollbar on the right edge of the table
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    let mut scrollbar_state = ScrollbarState::new(months.len())
        .position(state.monthly_table.selected().unwrap_or(0));
    f.render_stateful_widget(scrollbar, table_area, &mut scrollbar_state);
}

// ── Region + service tables ───────────────────────────────────────────────────

fn render_region_service(f: &mut Frame, area: Rect, results: &[EmissionsResult]) {
    let total_lbm: f64 = results.iter().map(|r| r.lbm).sum();

    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(area);

    render_ranked_table(f, halves[0], results, " Top Regions ", total_lbm, 10, |r| r.region.clone());
    render_ranked_table(f, halves[1], results, " Top Services ", total_lbm, 15, |r| r.service.clone());
}

fn render_ranked_table(
    f: &mut Frame,
    area: Rect,
    results: &[EmissionsResult],
    title: &str,
    total: f64,
    n: usize,
    key: impl Fn(&EmissionsResult) -> String,
) {
    let by_key = sum_by(results, &key, |r| r.lbm);
    let ranked: Vec<_> = top_n(&by_key, n).into_iter().filter(|(_, v)| *v > 0.0).collect();

    let rows: Vec<Row> = ranked
        .iter()
        .map(|(label, val)| {
            Row::new(vec![
                Cell::from(label.as_str()),
                Cell::from(fmt_co2(*val)),
                Cell::from(fmt_pct(*val, total)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [Constraint::Min(16), Constraint::Length(16), Constraint::Length(7)],
    )
    .header(Row::new(["Name", "LBM", "%"]).bold())
    .block(Block::default().title(title).borders(Borders::ALL));

    f.render_widget(table, area);
}

// ── Service × Month heatmap ───────────────────────────────────────────────────

fn render_heatmap(f: &mut Frame, area: Rect, results: &[EmissionsResult]) {
    let lbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.lbm);
    let months: Vec<String> = lbm_by_month.keys().cloned().collect();

    let by_service = sum_by(results, |r| r.service.clone(), |r| r.lbm);
    let top_services: Vec<String> = top_n(&by_service, 8).into_iter().map(|(s, _)| s).collect();

    // Sum lbm across all regions for each (service, month) pair
    let cell_map = sum_by(results, |r| (r.service.clone(), r.month.clone()), |r| r.lbm);

    let heatmap_rows: Vec<(String, Vec<f64>)> = top_services
        .iter()
        .map(|svc| {
            let vals = months
                .iter()
                .map(|m| *cell_map.get(&(svc.clone(), m.clone())).unwrap_or(&0.0))
                .collect();
            (svc.clone(), vals)
        })
        .collect();

    // Show only the month part (MM) as column labels so they fit in narrow cells
    let col_labels: Vec<String> = months
        .iter()
        .map(|m| m.get(5..).unwrap_or(m).to_string())
        .collect();

    f.render_widget(
        Heatmap::new(" Service × Month (LBM) ", heatmap_rows, col_labels),
        area,
    );
}
