use std::collections::HashSet;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState, Widget},
    Frame,
};

use crate::aws::EmissionsResult;
use crate::summary::{fmt_cell, fmt_co2, fmt_pct, sum_by, top_n};

// ── Heatmap widget ───────────────────────────────────────────────────────────

pub struct Heatmap<'a> {
    /// row label, then one value per column
    rows: Vec<(String, Vec<f64>)>,
    col_labels: Vec<String>,
    title: &'a str,
    /// Optional totals row rendered at the bottom with its own color scale
    total_row: Option<(String, Vec<f64>)>,
    selected_col: Option<usize>,
}

impl<'a> Heatmap<'a> {
    pub fn new(title: &'a str, rows: Vec<(String, Vec<f64>)>, col_labels: Vec<String>) -> Self {
        Self { rows, col_labels, title, total_row: None, selected_col: None }
    }

    pub fn total_row(mut self, label: String, values: Vec<f64>) -> Self {
        self.total_row = Some((label, values));
        self
    }

    pub fn selected_col(mut self, col: Option<usize>) -> Self {
        self.selected_col = col;
        self
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

        let total_max = self
            .total_row
            .iter()
            .flat_map(|(_, vals)| vals.iter().copied())
            .fold(0.0_f64, f64::max);

        // Column width: distribute inner width evenly across label + cols
        let n_cols = self.col_labels.len();
        if n_cols == 0 {
            return;
        }
        let label_w = self
            .rows
            .iter()
            .chain(self.total_row.iter())
            .map(|(l, _)| l.len())
            .max()
            .unwrap_or(0)
            .max(8) as u16;
        let cell_w = ((inner.width.saturating_sub(label_w + 1)) / n_cols as u16).max(1);

        // Header row (col labels)
        let mut x = inner.x + label_w + 1;
        for (col_i, label) in self.col_labels.iter().enumerate() {
            let truncated: String = label.chars().take(cell_w as usize).collect();
            let style = if self.selected_col == Some(col_i) {
                Style::default().bold().fg(Color::Yellow)
            } else {
                Style::default().bold()
            };
            buf.set_string(x, inner.y, &truncated, style);
            x += cell_w;
        }

        let selected_col = self.selected_col;
        let render_row = |buf: &mut Buffer, y: u16, row_label: &str, values: &[f64], row_max: f64, bold_label: bool| {
            let truncated: String = row_label.chars().take(label_w as usize).collect();
            let label_style = if bold_label { Style::default().bold() } else { Style::default() };
            buf.set_string(inner.x, y, &truncated, label_style);

            let mut x = inner.x + label_w + 1;
            for (col_i, &val) in values.iter().enumerate() {
                let intensity = if row_max > 0.0 { val / row_max } else { 0.0 };
                let bg = heat_color(intensity);
                let fg = if intensity > 0.55 { Color::Black } else { Color::White };
                let cell_label = fmt_cell(val, cell_w as usize);
                let padded = format!("{:width$}", cell_label, width = cell_w as usize);
                let style = if selected_col == Some(col_i) {
                    Style::default().fg(fg).bg(bg).bold()
                } else {
                    Style::default().fg(fg).bg(bg)
                };
                buf.set_string(x, y, &padded, style);
                x += cell_w;
            }
        };

        // Data rows
        let mut row_idx = 0usize;
        for (row_label, values) in &self.rows {
            let y = inner.y + 1 + row_idx as u16;
            if y >= inner.y + inner.height {
                break;
            }
            render_row(buf, y, row_label, values, max, false);
            row_idx += 1;
        }

        // Separator + total row
        if let Some((total_label, total_values)) = &self.total_row {
            // skip blank separator row
            row_idx += 1;
            let y = inner.y + 1 + row_idx as u16;
            if y < inner.y + inner.height {
                render_row(buf, y, total_label, total_values, total_max, true);
            }
        }
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
    pub selected_month_idx: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        let mut monthly_table = TableState::default();
        monthly_table.select(Some(0));
        Self { monthly_table, monthly_len: 0, selected_month_idx: None }
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
            Constraint::Length(7),  // header
            Constraint::Length(16), // chart (full width)
            Constraint::Length(14), // monthly table + top regions + top services
            Constraint::Min(10),    // heatmap
        ])
        .split(f.area());

    state.selected_month_idx = state.monthly_table.selected();

    render_header(f, chunks[0], results, title);
    render_chart(f, chunks[1], results, state);
    render_tables_row(f, chunks[2], results, state);
    render_heatmap(f, chunks[3], results, state);
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

// ── Monthly chart (full width) ───────────────────────────────────────────────

fn render_chart(f: &mut Frame, area: Rect, results: &[EmissionsResult], state: &AppState) {
    let lbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.lbm);
    let mbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.mbm);
    let months: Vec<String> = lbm_by_month.keys().cloned().collect();
    let n = months.len();

    let lbm_data: Vec<(f64, f64)> = months.iter().enumerate()
        .map(|(i, m)| (i as f64, lbm_by_month.get(m).copied().unwrap_or(0.0)))
        .collect();
    let mbm_data: Vec<(f64, f64)> = months.iter().enumerate()
        .map(|(i, m)| (i as f64, mbm_by_month.get(m).copied().unwrap_or(0.0)))
        .collect();

    let y_max = lbm_data.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max) * 1.1;

    let marker_data: Vec<(f64, f64)> = state.selected_month_idx
        .map(|i| vec![(i as f64, 0.0), (i as f64, y_max)])
        .unwrap_or_default();

    let mut datasets = vec![
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&lbm_data),
        Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Blue))
            .data(&mbm_data),
    ];
    if !marker_data.is_empty() {
        datasets.push(
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(Color::Yellow))
                .data(&marker_data),
        );
    }

    let chart = Chart::new(datasets)
        .block(Block::default().title(" Monthly LBM + MBM ").borders(Borders::ALL))
        .x_axis(
            Axis::default()
                .bounds([0.0, (n as f64) - 1.0]),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, y_max])
                .labels(vec![
                    Line::from("0"),
                    Line::from(format!("{:.1}", y_max / 2.0)),
                    Line::from(format!("{:.1}", y_max)),
                ]),
        );
    f.render_widget(chart, area);
}

// ── Monthly table + Top Regions + Top Services row ───────────────────────────

fn render_tables_row(f: &mut Frame, area: Rect, results: &[EmissionsResult], state: &mut AppState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 3), Constraint::Ratio(1, 3), Constraint::Ratio(1, 3)])
        .split(area);

    render_monthly_table(f, cols[0], results, state);

    let total_lbm: f64 = results.iter().map(|r| r.lbm).sum();
    render_ranked_table(f, cols[1], results, " Top Regions ", total_lbm, 10, |r| r.region.clone());
    render_ranked_table(f, cols[2], results, " Top Services ", total_lbm, 10, |r| r.service.clone());
}

fn render_monthly_table(f: &mut Frame, area: Rect, results: &[EmissionsResult], state: &mut AppState) {
    let lbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.lbm);
    let mbm_by_month = sum_by(results, |r| r.month.clone(), |r| r.mbm);
    let months: Vec<String> = lbm_by_month.keys().cloned().collect();

    state.monthly_len = months.len();

    let rows: Vec<Row> = months.iter().map(|m| {
        let lbm = lbm_by_month.get(m).copied().unwrap_or(0.0);
        let mbm = mbm_by_month.get(m).copied().unwrap_or(0.0);
        Row::new(vec![
            Cell::from(m.as_str()),
            Cell::from(fmt_co2(lbm)),
            Cell::from(fmt_co2(mbm)),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [Constraint::Length(8), Constraint::Fill(1), Constraint::Fill(1)],
    )
    .header(Row::new(["Month", "LBM", "MBM"]).bold())
    .row_highlight_style(Style::default().bold())
    .block(Block::default().title(" Monthly Values (↑↓) ").borders(Borders::ALL));

    f.render_stateful_widget(table, area, &mut state.monthly_table);

    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
    let mut scrollbar_state = ScrollbarState::new(months.len())
        .position(state.monthly_table.selected().unwrap_or(0));
    f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
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

fn render_heatmap(f: &mut Frame, area: Rect, results: &[EmissionsResult], state: &AppState) {
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

    // Total row: sum of all services (not just top 8) per month
    let total_vals: Vec<f64> = months
        .iter()
        .map(|m| lbm_by_month.get(m).copied().unwrap_or(0.0))
        .collect();

    // Show only the month part (MM) as column labels so they fit in narrow cells
    let col_labels: Vec<String> = months
        .iter()
        .map(|m| m.get(5..).unwrap_or(m).to_string())
        .collect();

    f.render_widget(
        Heatmap::new(" Service × Month (LBM) ", heatmap_rows, col_labels)
            .total_row("Total".to_string(), total_vals)
            .selected_col(state.selected_month_idx),
        area,
    );
}
