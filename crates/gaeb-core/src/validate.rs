use rust_decimal::Decimal;

use crate::{Document, SourceVersion, ValidationOptions};

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ValidationIssue {
    pub severity: ValidationSeverity,
    pub code: &'static str,
    pub message: String,
    pub location: Option<String>,
}

pub fn validate(document: &Document, options: &ValidationOptions) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let meta = document.meta();
    if meta.phase == crate::ExchangePhase::X95 {
        issues.push(error(
            "unsupported-phase",
            "X95 is not a released GAEB DA XML phase",
            None,
        ));
    } else if meta.version == SourceVersion::DaXml33 && !meta.phase.is_current_da_xml_33() {
        issues.push(error(
            "unsupported-phase-for-version",
            "X88 is legacy and is not defined by GAEB DA XML 3.3",
            None,
        ));
    }
    if meta.phase.kind() != document.kind() {
        issues.push(error(
            "phase-kind-mismatch",
            format!(
                "{} does not describe a {:?} document",
                meta.phase.code(),
                document.kind()
            ),
            None,
        ));
    }
    if options.strict_phase_profiles
        && !matches!(
            meta.version,
            SourceVersion::DaXml20
                | SourceVersion::DaXml21
                | SourceVersion::DaXml30
                | SourceVersion::Gaeb90
        )
        && !phase_profile_supported(meta.version, meta.phase)
    {
        issues.push(error(
            "unsupported-phase-profile",
            format!(
                "{} is not published in the {} phase profile",
                meta.phase.code(),
                meta.version.number()
            ),
            None,
        ));
    }
    match document {
        Document::Procurement(doc) => validate_procurement(doc, options, &mut issues),
        Document::Trade(doc) => {
            for item in &doc.items {
                if item.id.is_empty() {
                    issues.push(error(
                        "missing-item-id",
                        "trade item has no identifier",
                        None,
                    ));
                }
            }
        }
        Document::Cost(doc) => {
            for item in &doc.items {
                validate_cost(item, &mut issues);
            }
        }
        Document::Quantity(doc) => {
            for item in &doc.items {
                if item.oz.is_empty() {
                    issues.push(error("missing-oz", "quantity item has no OZ", None));
                }
            }
        }
        Document::Structured(_) => {}
    }
    if !options.suppress.is_empty() {
        let patterns: Vec<_> = options
            .suppress
            .iter()
            .filter_map(|pattern| {
                let has_metachar = pattern
                    .chars()
                    .any(|character| ".^$*+?{}[]\\|()".contains(character));
                let expression = if has_metachar {
                    pattern.clone()
                } else {
                    format!(r"\b{}\b", regex::escape(pattern))
                };
                regex::Regex::new(&expression).ok()
            })
            .collect();
        issues.retain(|issue| {
            !patterns
                .iter()
                .any(|pattern| pattern.is_match(&issue.message))
        });
    }
    issues
}

fn phase_profile_supported(version: SourceVersion, phase: crate::ExchangePhase) -> bool {
    match version {
        SourceVersion::DaXml31 => matches!(
            phase,
            crate::ExchangePhase::X81
                | crate::ExchangePhase::X82
                | crate::ExchangePhase::X83
                | crate::ExchangePhase::X84
                | crate::ExchangePhase::X85
                | crate::ExchangePhase::X86
                | crate::ExchangePhase::X87
                | crate::ExchangePhase::X88
                | crate::ExchangePhase::X93
                | crate::ExchangePhase::X94
                | crate::ExchangePhase::X96
                | crate::ExchangePhase::X97
        ),
        SourceVersion::DaXml32 => matches!(
            phase,
            crate::ExchangePhase::X31
                | crate::ExchangePhase::X52
                | crate::ExchangePhase::X80
                | crate::ExchangePhase::X81
                | crate::ExchangePhase::X82
                | crate::ExchangePhase::X83
                | crate::ExchangePhase::X84
                | crate::ExchangePhase::X85
                | crate::ExchangePhase::X86
                | crate::ExchangePhase::X87
                | crate::ExchangePhase::X89
                | crate::ExchangePhase::X93
                | crate::ExchangePhase::X94
                | crate::ExchangePhase::X96
                | crate::ExchangePhase::X97
                | crate::ExchangePhase::X83Z
                | crate::ExchangePhase::X84Z
                | crate::ExchangePhase::X86ZR
                | crate::ExchangePhase::X86ZE
        ),
        SourceVersion::DaXml33 => phase.is_current_da_xml_33(),
        SourceVersion::DaXml20
        | SourceVersion::DaXml21
        | SourceVersion::DaXml30
        | SourceVersion::Gaeb90 => true,
    }
}

fn validate_procurement(
    doc: &crate::ProcurementDocument,
    options: &ValidationOptions,
    issues: &mut Vec<ValidationIssue>,
) {
    let mut total = Decimal::ZERO;
    for item in doc.boq.items() {
        if item.oz.is_empty() {
            issues.push(error("missing-oz", "procurement item has no RNoPart", None));
        }
        if item.quantity_tbd && item.quantity.is_some() {
            issues.push(warning(
                "quantity-tbd-with-value",
                format!("{} has QtyTBD and Qty", item.oz),
                Some(item.oz.clone()),
            ));
        }
        if item.item_type == crate::ItemType::Supplement && item.change_order_number.is_none() {
            issues.push(warning(
                "supplement-missing-change-order-number",
                format!("{} is a supplement item without CONo", item.oz),
                Some(item.oz.clone()),
            ));
        }
        if !item.qty_splits.is_empty()
            && let Some(quantity) = item.quantity
        {
            let split_total: Decimal = item.qty_splits.iter().map(|split| split.quantity).sum();
            if (split_total - quantity).abs() > Decimal::new(1, 3) {
                issues.push(warning(
                    "quantity-split-mismatch",
                    format!(
                        "{} QtySplit total {split_total} differs from quantity {quantity}",
                        item.oz
                    ),
                    Some(item.oz.clone()),
                ));
            }
        }
        if item.up_components.len() > 6 {
            issues.push(warning(
                "too-many-unit-price-components",
                format!("{} has more than six UPComp values", item.oz),
                Some(item.oz.clone()),
            ));
        }
        if let (Some(expected), Some(actual)) = (item.computed_total(), item.total_price)
            && expected.round_dp(2) != actual.round_dp(2)
        {
            issues.push(warning(
                "total-mismatch",
                format!(
                    "{} total {actual} differs from quantity times unit price {expected}",
                    item.oz
                ),
                Some(item.oz.clone()),
            ));
        }
        if item.item_type.affects_total() {
            total += item.total_price.unwrap_or(Decimal::ZERO);
        }
    }
    if options.check_totals
        && let Some(declared) = doc.boq.totals
        && declared != total
    {
        issues.push(warning(
            "boq-total-mismatch",
            format!("BoQ total {declared} differs from item total {total}"),
            None,
        ));
    }
}

fn validate_cost(item: &crate::CostItem, issues: &mut Vec<ValidationIssue>) {
    if item.id.is_empty() {
        issues.push(error(
            "missing-element-id",
            "cost element has no identifier",
            None,
        ));
    }
    if let (Some(expected), Some(actual)) = (item.computed_total(), item.total_price)
        && expected.round_dp(2) != actual.round_dp(2)
    {
        issues.push(warning(
            "cost-total-mismatch",
            format!(
                "{} total {actual} differs from quantity times unit price {expected}",
                item.id
            ),
            Some(item.id.clone()),
        ));
    }
    for child in &item.children {
        validate_cost(child, issues);
    }
}

fn error(
    code: &'static str,
    message: impl Into<String>,
    location: Option<String>,
) -> ValidationIssue {
    ValidationIssue {
        severity: ValidationSeverity::Error,
        code,
        message: message.into(),
        location,
    }
}
fn warning(
    code: &'static str,
    message: impl Into<String>,
    location: Option<String>,
) -> ValidationIssue {
    ValidationIssue {
        severity: ValidationSeverity::Warning,
        code,
        message: message.into(),
        location,
    }
}
