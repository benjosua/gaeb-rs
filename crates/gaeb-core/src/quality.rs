use crate::{Document, DocumentItem, ExchangePhase, ValidationSeverity};

#[derive(Clone, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct QualityScore {
    pub overall: u8,
    pub completeness: u8,
    pub precision: u8,
    pub structure: u8,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

pub fn quality_score(document: &Document) -> QualityScore {
    let issues = document.validation_issues();
    let error_count = issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Warning)
        .count();
    let info_count = issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Info)
        .count();
    let item_count = document.items().count();

    let complete_count = document
        .items()
        .filter(|item| is_complete(item, document.phase()))
        .count();
    let completeness = percentage(complete_count, item_count);

    let precision_warnings = issues
        .iter()
        .filter(|issue| {
            let message = issue.message.to_ascii_lowercase();
            message.contains("precision")
                || message.contains("rounding")
                || message.contains("mismatch")
        })
        .count();
    let precision = (100 * precision_warnings)
        .checked_div(item_count)
        .map_or(100, |penalty| 100u8.saturating_sub(penalty.min(100) as u8));

    let structural_errors = issues
        .iter()
        .filter(|issue| {
            issue.severity == ValidationSeverity::Error && {
                let message = issue.message.to_ascii_lowercase();
                message.contains("structure")
                    || message.contains("missing")
                    || message.contains("duplicate")
            }
        })
        .count();
    let structure = 100u8.saturating_sub((structural_errors * 10).min(100) as u8);
    let overall =
        ((u16::from(completeness) + u16::from(precision) + u16::from(structure)) / 3) as u8;

    QualityScore {
        overall,
        completeness,
        precision,
        structure,
        error_count,
        warning_count,
        info_count,
    }
}

fn percentage(found: usize, total: usize) -> u8 {
    (100 * found)
        .checked_div(total)
        .map_or(100, |value| value.min(100) as u8)
}

fn is_complete(item: &DocumentItem<'_>, phase: ExchangePhase) -> bool {
    match item {
        DocumentItem::Procurement(item) => {
            if matches!(item.item_type, crate::ItemType::Markup) {
                return true;
            }
            let has_content = !item.short_text.is_empty()
                && (item.quantity.is_some() || item.unit_price.is_some());
            let price_expected = matches!(
                phase,
                ExchangePhase::X52
                    | ExchangePhase::X80
                    | ExchangePhase::X81
                    | ExchangePhase::X82
                    | ExchangePhase::X84
                    | ExchangePhase::X85
                    | ExchangePhase::X86
                    | ExchangePhase::X87
            );
            has_content
                && (!price_expected || !item.item_type.affects_total() || item.unit_price.is_some())
        }
        DocumentItem::Trade(item) => !item.short_text.is_empty() && item.quantity.is_some(),
        DocumentItem::Cost(item) => {
            !item.short_text.is_empty() && (item.quantity.is_some() || item.unit_price.is_some())
        }
        DocumentItem::Quantity(item) => !item.oz.is_empty(),
    }
}
