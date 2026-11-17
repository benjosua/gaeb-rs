use crate::{Document, DocumentItem, GaebError};

pub fn to_json_string(document: &Document, pretty: bool) -> Result<String, GaebError> {
    if pretty {
        serde_json::to_string_pretty(document)
            .map_err(|error| GaebError::Unsupported(error.to_string()))
    } else {
        serde_json::to_string(document).map_err(|error| GaebError::Unsupported(error.to_string()))
    }
}

pub fn to_csv_string(document: &Document, delimiter: char) -> String {
    let mut output = String::from("identifier,short_text,quantity,unit,unit_price,total_price\n");
    for item in document.items() {
        let (identifier, short_text, quantity, unit, unit_price, total_price) = match item {
            DocumentItem::Procurement(item) => (
                item.full_oz(),
                item.short_text.clone(),
                item.quantity.map(|value| value.to_string()),
                item.unit.clone().unwrap_or_default(),
                item.unit_price.map(|value| value.to_string()),
                item.total_price.map(|value| value.to_string()),
            ),
            DocumentItem::Trade(item) => (
                item.id.clone(),
                item.short_text.clone(),
                item.quantity.map(|value| value.to_string()),
                item.unit.clone().unwrap_or_default(),
                item.net_price
                    .or(item.offer_price)
                    .map(|value| value.to_string()),
                None,
            ),
            DocumentItem::Cost(item) => (
                item.id.clone(),
                item.short_text.clone(),
                item.quantity.map(|value| value.to_string()),
                item.unit.clone().unwrap_or_default(),
                item.unit_price.map(|value| value.to_string()),
                item.total_price.map(|value| value.to_string()),
            ),
            DocumentItem::Quantity(item) => (
                item.oz.clone(),
                String::new(),
                item.quantity.map(|value| value.to_string()),
                String::new(),
                None,
                None,
            ),
        };
        let fields = [
            identifier,
            short_text,
            quantity.unwrap_or_default(),
            unit,
            unit_price.unwrap_or_default(),
            total_price.unwrap_or_default(),
        ];
        let escaped: Vec<_> = fields
            .iter()
            .map(|field| csv_field(field, delimiter))
            .collect();
        output.push_str(&escaped.join(&delimiter.to_string()));
        output.push('\n');
    }
    output
}

/// Render a diff as a standalone report with no filesystem or HTML dependency.
pub fn diff_to_html_string(result: &crate::diff::DiffResult) -> String {
    let mut rows = String::new();
    for item in &result.items.added {
        rows.push_str(&format!(
            "<tr class=\"added\"><td>+ ADDED</td><td>{}</td><td>{}</td><td>-</td><td></td></tr>",
            html_escape(&item.oz),
            html_escape(&item.short_text)
        ));
    }
    for item in &result.items.removed {
        rows.push_str(&format!(
            "<tr class=\"removed\"><td>- REMOVED</td><td>{}</td><td>{}</td><td>-</td><td></td></tr>",
            html_escape(&item.oz),
            html_escape(&item.short_text)
        ));
    }
    for item in &result.items.modified {
        let changes = item
            .changes
            .iter()
            .map(|change| {
                format!(
                    "{}: {} -> {}",
                    html_escape(&change.field),
                    html_escape(&change.old_value),
                    html_escape(&change.new_value)
                )
            })
            .collect::<Vec<_>>()
            .join("<br>");
        rows.push_str(&format!(
            "<tr class=\"modified\"><td>~ MODIFIED</td><td>{}</td><td>{}</td><td>{:?}</td><td>{}</td></tr>",
            html_escape(&item.oz),
            html_escape(&item.short_text_b),
            item.changes.iter().map(|change| change.significance).min_by_key(|value| value.rank()).unwrap_or(crate::diff::Significance::Low),
            changes
        ));
    }
    if rows.is_empty() {
        rows.push_str("<tr><td colspan=\"5\">No item-level changes detected.</td></tr>");
    }
    format!(
        "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>GAEB Diff Report</title><style>body{{font-family:sans-serif;margin:2em;color:#222}}table{{border-collapse:collapse;width:100%}}th,td{{padding:.5em;border-bottom:1px solid #ddd;text-align:left}}th{{background:#333;color:white}}.added td{{background:#e6ffec}}.removed td{{background:#ffeef0}}.modified td{{background:#fff8c5}}</style></head><body><h1>GAEB Diff Report</h1><p>Total changes: {} | Added: {} | Removed: {} | Modified: {} | Match ratio: {:.1}%</p><table><thead><tr><th>Status</th><th>OZ</th><th>Description</th><th>Significance</th><th>Changes</th></tr></thead><tbody>{}</tbody></table></body></html>",
        result.summary.total_changes,
        result.summary.items_added,
        result.summary.items_removed,
        result.summary.items_modified,
        result.summary.match_ratio * 100.0,
        rows
    )
}

/// Export the same three logical report sections as an Excel-compatible CSV.
/// Consumers can open this directly in spreadsheet software without a binary
/// workbook dependency in the core crate.
pub fn diff_to_excel_csv_string(result: &crate::diff::DiffResult) -> String {
    let mut output = String::from("[Summary]\nmetric,value\n");
    for (name, value) in [
        ("Total changes", result.summary.total_changes.to_string()),
        ("Items added", result.summary.items_added.to_string()),
        ("Items removed", result.summary.items_removed.to_string()),
        ("Items modified", result.summary.items_modified.to_string()),
        (
            "Items unchanged",
            result.summary.items_unchanged.to_string(),
        ),
        (
            "Match ratio",
            format!("{:.1}%", result.summary.match_ratio * 100.0),
        ),
    ] {
        output.push_str(&format!(
            "{},{}\n",
            csv_field(name, ','),
            csv_field(&value, ',')
        ));
    }
    output.push_str("\n[Items]\nStatus,OZ,Description,Significance,Field changes\n");
    for item in &result.items.added {
        output.push_str(&format!(
            "ADDED,{},{},,\n",
            csv_field(&item.oz, ','),
            csv_field(&item.short_text, ',')
        ));
    }
    for item in &result.items.removed {
        output.push_str(&format!(
            "REMOVED,{},{},,\n",
            csv_field(&item.oz, ','),
            csv_field(&item.short_text, ',')
        ));
    }
    for item in &result.items.modified {
        let changes = item
            .changes
            .iter()
            .map(|change| {
                format!(
                    "{}: {} -> {}",
                    change.field, change.old_value, change.new_value
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        output.push_str(&format!(
            "MODIFIED,{},{},{:?},{}\n",
            csv_field(&item.oz, ','),
            csv_field(&item.short_text_b, ','),
            item.changes
                .iter()
                .map(|change| change.significance)
                .min_by_key(|value| value.rank())
                .unwrap_or(crate::diff::Significance::Low),
            csv_field(&changes, ',')
        ));
    }
    output.push_str("\n[Structure]\nSection,Change,Details\n");
    for section in &result.structure.sections_added {
        output.push_str(&format!(
            "{},ADDED,{}\n",
            csv_field(&section.rno, ','),
            csv_field(&section.label, ',')
        ));
    }
    for section in &result.structure.sections_removed {
        output.push_str(&format!(
            "{},REMOVED,{}\n",
            csv_field(&section.rno, ','),
            csv_field(&section.label, ',')
        ));
    }
    for section in &result.structure.sections_renamed {
        output.push_str(&format!(
            "{},RENAMED,{} -> {}\n",
            csv_field(&section.rno, ','),
            csv_field(&section.old_label, ','),
            csv_field(&section.new_label, ',')
        ));
    }
    for item in &result.structure.items_moved {
        output.push_str(&format!(
            "{},MOVED,{} -> {}\n",
            csv_field(&item.oz, ','),
            csv_field(&item.old_category_rno, ','),
            csv_field(&item.new_category_rno, ',')
        ));
    }
    output
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn csv_field(value: &str, delimiter: char) -> String {
    if value.contains(['"', '\n', '\r', delimiter]) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}
