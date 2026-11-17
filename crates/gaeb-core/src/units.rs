use crate::{Document, Item};

pub fn normalize_unit(unit: Option<&str>) -> Option<String> {
    let unit = unit?;
    let cleaned = unit.trim();
    if cleaned.is_empty() {
        return Some(String::new());
    }
    let canonical = match cleaned.to_ascii_lowercase().as_str() {
        "m2" | "m^2" | "qm" | "qm." | "sqm" | "qm2" | "m**2" | "m²" => "m²",
        "m3" | "m^3" | "cbm" | "cbm." | "m**3" | "m³" => "m³",
        "lfdm" | "lfdm." | "lm" | "lfd.m" | "lfm" => "lfm",
        "m" => "m",
        "stk" | "stck" | "stk." | "stck." | "st" | "st." | "pcs" | "pieces" => "Stk",
        "psch" | "psch." | "pausch" | "pauschal" | "ls" | "lump" => "psch",
        "kg" => "kg",
        "t" | "to" | "tonne" | "tonnen" => "t",
        "h" | "std" | "std." | "hr" => "h",
        _ => return Some(cleaned.to_owned()),
    };
    Some(canonical.to_owned())
}

pub fn normalize_units_in_document(document: &mut Document) -> usize {
    let mut changed = 0;
    match document {
        Document::Procurement(document) => {
            for item in document.boq.items_mut() {
                changed += normalize_item_unit(item);
            }
        }
        Document::Trade(document) => {
            for item in &mut document.items {
                if let Some(normalized) = normalize_unit(item.unit.as_deref())
                    && item.unit.as_deref() != Some(normalized.as_str())
                {
                    item.unit = Some(normalized);
                    changed += 1;
                }
            }
        }
        Document::Cost(document) => {
            for item in &mut document.items {
                changed += normalize_cost_unit(item);
            }
        }
        Document::Quantity(_) | Document::Structured(_) => {}
    }
    changed
}

fn normalize_item_unit(item: &mut Item) -> usize {
    let Some(normalized) = normalize_unit(item.unit.as_deref()) else {
        return 0;
    };
    if item.unit.as_deref() == Some(&normalized) {
        0
    } else {
        item.unit = Some(normalized);
        1
    }
}

fn normalize_cost_unit(item: &mut crate::CostItem) -> usize {
    let mut changed = 0;
    if let Some(normalized) = normalize_unit(item.unit.as_deref())
        && item.unit.as_deref() != Some(normalized.as_str())
    {
        item.unit = Some(normalized);
        changed += 1;
    }
    for child in &mut item.children {
        changed += normalize_cost_unit(child);
    }
    changed
}
