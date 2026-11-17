use crate::model::{Boq, BoqCategory, Document, Item, ItemType};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Significance {
    Critical,
    High,
    Medium,
    Low,
}

impl Significance {
    pub(crate) fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Low => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    #[default]
    Default,
    Strict,
    Force,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub significance: Significance,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub absolute_delta: Option<Decimal>,
    pub percent_delta: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemModified {
    pub oz: String,
    pub short_text_a: String,
    pub short_text_b: String,
    pub lot_rno: String,
    pub changes: Vec<FieldChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemAdded {
    pub oz: String,
    pub short_text: String,
    pub lot_rno: String,
    pub category_rno: String,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub total_price: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemRemoved {
    pub oz: String,
    pub short_text: String,
    pub lot_rno: String,
    pub category_rno: String,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub total_price: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemMoved {
    pub oz: String,
    pub short_text: String,
    pub old_category_rno: String,
    pub new_category_rno: String,
    pub old_lot_rno: String,
    pub new_lot_rno: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionChange {
    pub rno: String,
    pub label: String,
    pub lot_rno: String,
    pub item_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionRenamed {
    pub rno: String,
    pub old_label: String,
    pub new_label: String,
    pub lot_rno: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StructureDiffSummary {
    pub sections_added: Vec<SectionChange>,
    pub sections_removed: Vec<SectionChange>,
    pub sections_renamed: Vec<SectionRenamed>,
    pub items_moved: Vec<ItemMoved>,
}

impl StructureDiffSummary {
    pub fn has_changes(&self) -> bool {
        !self.sections_added.is_empty()
            || !self.sections_removed.is_empty()
            || !self.sections_renamed.is_empty()
            || !self.items_moved.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemDiffSummary {
    pub added: Vec<ItemAdded>,
    pub removed: Vec<ItemRemoved>,
    pub modified: Vec<ItemModified>,
    pub unchanged_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetadataChange {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiffDocInfo {
    pub source_version: String,
    pub exchange_phase: String,
    pub project_no: Option<String>,
    pub project_name: Option<String>,
    pub currency: Option<String>,
    pub item_count: usize,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub grand_total: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateOz {
    pub oz: String,
    pub count_a: usize,
    pub count_b: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub has_changes: bool,
    pub total_changes: usize,
    pub items_added: usize,
    pub items_removed: usize,
    pub items_modified: usize,
    pub items_unchanged: usize,
    pub match_ratio: f64,
    pub is_likely_same_project: bool,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub financial_impact: Option<Decimal>,
    pub max_significance: Significance,
    pub duplicates_collapsed: Vec<DuplicateOz>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffResult {
    // Kept as top-level fields for compatibility with the original Rust API.
    pub added: Vec<ItemAdded>,
    pub removed: Vec<ItemRemoved>,
    pub modified: Vec<ItemModified>,
    #[serde(with = "rust_decimal::serde::str_option", default)]
    pub financial_impact: Option<Decimal>,
    pub doc_a: DiffDocInfo,
    pub doc_b: DiffDocInfo,
    pub summary: DiffSummary,
    pub items: ItemDiffSummary,
    pub structure: StructureDiffSummary,
    pub metadata: Vec<MetadataChange>,
    pub warnings: Vec<String>,
}

#[derive(Clone)]
struct LocatedItem<'a> {
    item: &'a Item,
    full_oz: String,
    category_rno: &'a str,
    category_path: String,
    lot_rno: &'a str,
}

fn collect_items<'a>(boq: &'a Boq) -> Vec<LocatedItem<'a>> {
    let mut result = Vec::new();
    if boq.lots.is_empty() {
        for category in &boq.categories {
            collect_category_items(category, &mut Vec::new(), "", &mut result);
        }
    } else {
        for lot in &boq.lots {
            for category in &lot.categories {
                collect_category_items(category, &mut Vec::new(), &lot.rno, &mut result);
            }
        }
    }
    result
}

fn collect_category_items<'a>(
    category: &'a BoqCategory,
    path: &mut Vec<String>,
    lot_rno: &'a str,
    result: &mut Vec<LocatedItem<'a>>,
) {
    if !category.rno.is_empty() {
        path.push(category.rno.clone());
    }
    for item in &category.items {
        result.push(LocatedItem {
            item,
            full_oz: item.full_oz(),
            category_rno: &category.rno,
            category_path: path.join("."),
            lot_rno,
        });
    }
    for child in &category.categories {
        collect_category_items(child, path, lot_rno, result);
    }
    if !category.rno.is_empty() {
        path.pop();
    }
}

fn category_index(boq: &Boq) -> BTreeMap<(String, String), (&BoqCategory, usize)> {
    let mut result = BTreeMap::new();
    if boq.lots.is_empty() {
        for category in &boq.categories {
            collect_categories(category, &mut Vec::new(), "", &mut result);
        }
    } else {
        for lot in &boq.lots {
            for category in &lot.categories {
                collect_categories(category, &mut Vec::new(), &lot.rno, &mut result);
            }
        }
    }
    result
}

fn collect_categories<'a>(
    category: &'a BoqCategory,
    path: &mut Vec<String>,
    lot_rno: &str,
    result: &mut BTreeMap<(String, String), (&'a BoqCategory, usize)>,
) {
    if !category.rno.is_empty() {
        path.push(category.rno.clone());
    }
    let key = path.join(".");
    if !key.is_empty() {
        result
            .entry((lot_rno.to_owned(), key))
            .or_insert((category, count_category_items(category)));
    }
    for child in &category.categories {
        collect_categories(child, path, lot_rno, result);
    }
    if !category.rno.is_empty() {
        path.pop();
    }
}

fn count_category_items(category: &BoqCategory) -> usize {
    category.items.len()
        + category
            .categories
            .iter()
            .map(count_category_items)
            .sum::<usize>()
}

fn procurement(document: &Document) -> Option<&crate::ProcurementDocument> {
    match document {
        Document::Procurement(value) => Some(value),
        _ => None,
    }
}

pub fn compare(doc_a: &Document, doc_b: &Document) -> DiffResult {
    compare_with_mode(doc_a, doc_b, DiffMode::Default).expect("default diff mode cannot fail")
}

pub fn compare_with_mode(
    doc_a: &Document,
    doc_b: &Document,
    mode: DiffMode,
) -> Result<DiffResult, crate::GaebError> {
    let a = procurement(doc_a).ok_or_else(|| {
        crate::GaebError::Unsupported("BoQ diff supports procurement documents only".to_owned())
    })?;
    let b = procurement(doc_b).ok_or_else(|| {
        crate::GaebError::Unsupported("BoQ diff supports procurement documents only".to_owned())
    })?;
    let info_a = doc_info(a);
    let info_b = doc_info(b);
    let project_mismatch = info_a.project_no.is_some()
        && info_b.project_no.is_some()
        && info_a.project_no != info_b.project_no;
    if mode == DiffMode::Strict && project_mismatch {
        return Err(crate::GaebError::Unsupported(format!(
            "Documents appear to be from different projects ({:?} vs {:?})",
            info_a.project_no, info_b.project_no
        )));
    }

    let items_a = collect_items(&a.boq);
    let items_b = collect_items(&b.boq);
    let mut index_b = BTreeMap::new();
    let mut counts_a = BTreeMap::new();
    let mut counts_b = BTreeMap::new();
    for item in &items_a {
        *counts_a.entry(item.full_oz.clone()).or_insert(0) += 1;
    }
    for item in &items_b {
        *counts_b.entry(item.full_oz.clone()).or_insert(0) += 1;
        index_b
            .entry((item.lot_rno, item.full_oz.clone()))
            .or_insert(item.clone());
    }

    let mut seen_a = BTreeSet::new();
    let unique_a: Vec<_> = items_a
        .iter()
        .filter(|item| seen_a.insert((item.lot_rno, item.full_oz.clone())))
        .cloned()
        .collect();
    let mut seen_b = BTreeSet::new();
    let unique_b: Vec<_> = items_b
        .iter()
        .filter(|item| seen_b.insert((item.lot_rno, item.full_oz.clone())))
        .cloned()
        .collect();

    let mut matched = Vec::new();
    let mut used_b = BTreeSet::new();
    let mut unmatched_a = Vec::new();
    for item in &unique_a {
        let key = (item.lot_rno, item.full_oz.clone());
        if let Some(other) = index_b.get(&key) {
            if used_b.insert((other.lot_rno, other.full_oz.clone())) {
                matched.push((item.clone(), other.clone()));
            }
        } else {
            unmatched_a.push(item.clone());
        }
    }
    let mut unmatched_b: Vec<_> = unique_b
        .iter()
        .filter(|item| !used_b.contains(&(item.lot_rno, item.full_oz.clone())))
        .cloned()
        .collect();
    // Fallback by leaf OZ catches category/lot moves for programmatically built items.
    let mut fallback_b = BTreeMap::new();
    for item in &unmatched_b {
        fallback_b
            .entry(item.item.oz.clone())
            .or_insert(item.clone());
    }
    let mut moved_a = Vec::new();
    let mut fallback_used = BTreeSet::new();
    for item in unmatched_a {
        if let Some(other) = fallback_b.get(&item.item.oz) {
            if other.full_oz == item.full_oz
                && other.category_path == item.category_path
                && other.lot_rno == item.lot_rno
            {
                moved_a.push(item);
            } else if fallback_used.insert(other.item as *const Item) {
                matched.push((item, other.clone()));
                unmatched_b.retain(|candidate| !std::ptr::eq(candidate.item, other.item));
            } else {
                moved_a.push(item);
            }
        } else {
            moved_a.push(item);
        }
    }

    let mut added = Vec::new();
    for item in &unmatched_b {
        added.push(ItemAdded {
            oz: item.full_oz.clone(),
            short_text: item.item.short_text.clone(),
            lot_rno: item.lot_rno.to_owned(),
            category_rno: item.category_rno.to_owned(),
            total_price: item.item.total_price,
        });
    }
    let mut removed = Vec::new();
    for item in &moved_a {
        removed.push(ItemRemoved {
            oz: item.full_oz.clone(),
            short_text: item.item.short_text.clone(),
            lot_rno: item.lot_rno.to_owned(),
            category_rno: item.category_rno.to_owned(),
            total_price: item.item.total_price,
        });
    }
    let mut modified = Vec::new();
    let mut unchanged_count = 0;
    let mut moved = Vec::new();
    for (old, new) in matched {
        let changes = compare_items(old.item, new.item);
        if changes.is_empty() {
            unchanged_count += 1;
        } else {
            modified.push(ItemModified {
                oz: old.full_oz.clone(),
                short_text_a: old.item.short_text.clone(),
                short_text_b: new.item.short_text.clone(),
                lot_rno: old.lot_rno.to_owned(),
                changes,
            });
        }
        if old.category_path != new.category_path || old.lot_rno != new.lot_rno {
            moved.push(ItemMoved {
                oz: old.full_oz.clone(),
                short_text: old.item.short_text.clone(),
                old_category_rno: old.category_rno.to_owned(),
                new_category_rno: new.category_rno.to_owned(),
                old_lot_rno: old.lot_rno.to_owned(),
                new_lot_rno: new.lot_rno.to_owned(),
            });
        }
    }
    let matched_count = unchanged_count + modified.len();
    let denominator = (unique_a.len().max(unique_b.len()).max(1)) as f64;
    let match_ratio = matched_count as f64 / denominator;
    let structure = compare_structure(&a.boq, &b.boq, moved);
    let metadata = compare_metadata(&info_a, &info_b);
    let warnings = warnings(&info_a, &info_b, match_ratio, mode);
    let item_summary = ItemDiffSummary {
        added,
        removed,
        modified,
        unchanged_count,
    };
    let max_significance = item_summary
        .modified
        .iter()
        .flat_map(|item| item.changes.iter().map(|change| change.significance))
        .chain(item_summary.added.iter().map(|_| Significance::High))
        .chain(item_summary.removed.iter().map(|_| Significance::High))
        .min_by_key(|value| value.rank())
        .unwrap_or(Significance::Low);
    let total_a = a
        .boq
        .items()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price)
        .sum::<Decimal>();
    let total_b = b
        .boq
        .items()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price)
        .sum::<Decimal>();
    let financial_impact = Some(total_b - total_a);
    let duplicates_collapsed = counts_a
        .keys()
        .chain(counts_b.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|oz| {
            let count_a = *counts_a.get(oz).unwrap_or(&0);
            let count_b = *counts_b.get(oz).unwrap_or(&0);
            (count_a > 1 || count_b > 1).then_some(DuplicateOz {
                oz: oz.clone(),
                count_a: count_a.max(1),
                count_b: count_b.max(1),
            })
        })
        .collect();
    let summary = DiffSummary {
        has_changes: !item_summary.added.is_empty()
            || !item_summary.removed.is_empty()
            || !item_summary.modified.is_empty(),
        total_changes: item_summary.added.len()
            + item_summary.removed.len()
            + item_summary.modified.len(),
        items_added: item_summary.added.len(),
        items_removed: item_summary.removed.len(),
        items_modified: item_summary.modified.len(),
        items_unchanged: item_summary.unchanged_count,
        match_ratio,
        is_likely_same_project: same_project(&info_a, &info_b, match_ratio),
        financial_impact,
        max_significance,
        duplicates_collapsed,
    };
    Ok(DiffResult {
        added: item_summary.added.clone(),
        removed: item_summary.removed.clone(),
        modified: item_summary.modified.clone(),
        financial_impact,
        doc_a: info_a,
        doc_b: info_b,
        summary,
        items: item_summary,
        structure,
        metadata,
        warnings,
    })
}

fn doc_info(document: &crate::ProcurementDocument) -> DiffDocInfo {
    DiffDocInfo {
        source_version: document.meta.version.number().to_owned(),
        exchange_phase: document.meta.phase.code().to_owned(),
        project_no: document.project.id.clone(),
        project_name: document.project.name.clone(),
        currency: document.project.currency.clone(),
        item_count: document.boq.items().count(),
        grand_total: Some(
            document
                .boq
                .items()
                .filter(|item| item.item_type.affects_total())
                .filter_map(|item| item.total_price)
                .sum(),
        ),
    }
}

fn compare_metadata(a: &DiffDocInfo, b: &DiffDocInfo) -> Vec<MetadataChange> {
    let fields = [
        (
            "exchange_phase",
            a.exchange_phase.clone(),
            b.exchange_phase.clone(),
        ),
        (
            "source_version",
            a.source_version.clone(),
            b.source_version.clone(),
        ),
        (
            "currency",
            a.currency.clone().unwrap_or_default(),
            b.currency.clone().unwrap_or_default(),
        ),
        (
            "project_no",
            a.project_no.clone().unwrap_or_default(),
            b.project_no.clone().unwrap_or_default(),
        ),
        (
            "project_name",
            a.project_name.clone().unwrap_or_default(),
            b.project_name.clone().unwrap_or_default(),
        ),
    ];
    fields
        .into_iter()
        .filter_map(|(field, old_value, new_value)| {
            (old_value != new_value).then_some(MetadataChange {
                field: field.to_owned(),
                old_value,
                new_value,
            })
        })
        .collect()
}

fn warnings(a: &DiffDocInfo, b: &DiffDocInfo, ratio: f64, mode: DiffMode) -> Vec<String> {
    if mode == DiffMode::Force {
        return Vec::new();
    }
    let mut result = Vec::new();
    if a.project_no.is_some() && b.project_no.is_some() && a.project_no != b.project_no {
        result.push(format!(
            "Documents may be from different projects ({:?} vs {:?}).",
            a.project_no, b.project_no
        ));
    }
    if a.currency != b.currency {
        result.push(format!(
            "Currency changed from {:?} to {:?}. Financial comparisons may be misleading.",
            a.currency, b.currency
        ));
    }
    if ratio < 0.3 {
        result.push(format!(
            "Low match ratio ({:.0}%). Documents may be unrelated.",
            ratio * 100.0
        ));
    }
    if a.source_version != b.source_version {
        result.push(format!(
            "GAEB versions differ ({} vs {}).",
            a.source_version, b.source_version
        ));
    }
    result
}

fn same_project(a: &DiffDocInfo, b: &DiffDocInfo, ratio: f64) -> bool {
    if let (Some(old), Some(new)) = (&a.project_no, &b.project_no) {
        return old == new;
    }
    if let (Some(old), Some(new)) = (&a.project_name, &b.project_name) {
        return old.eq_ignore_ascii_case(new);
    }
    ratio >= 0.5
}

fn compare_structure(a: &Boq, b: &Boq, items_moved: Vec<ItemMoved>) -> StructureDiffSummary {
    let cats_a = category_index(a);
    let cats_b = category_index(b);
    let sections_added = cats_b
        .iter()
        .filter(|(key, _)| !cats_a.contains_key(*key))
        .map(|(key, (category, count))| SectionChange {
            rno: category.rno.clone(),
            label: category.label.clone(),
            lot_rno: key.0.clone(),
            item_count: *count,
        })
        .collect();
    let sections_removed = cats_a
        .iter()
        .filter(|(key, _)| !cats_b.contains_key(*key))
        .map(|(key, (category, count))| SectionChange {
            rno: category.rno.clone(),
            label: category.label.clone(),
            lot_rno: key.0.clone(),
            item_count: *count,
        })
        .collect();
    let sections_renamed = cats_a
        .iter()
        .filter_map(|(key, (old, _))| {
            cats_b.get(key).and_then(|(new, _)| {
                (old.label != new.label).then_some(SectionRenamed {
                    rno: old.rno.clone(),
                    old_label: old.label.clone(),
                    new_label: new.label.clone(),
                    lot_rno: key.0.clone(),
                })
            })
        })
        .collect();
    StructureDiffSummary {
        sections_added,
        sections_removed,
        sections_renamed,
        items_moved,
    }
}

fn compare_items(a: &Item, b: &Item) -> Vec<FieldChange> {
    let mut result = Vec::new();
    macro_rules! text_field {
        ($name:literal, $old:expr, $new:expr, $sig:expr) => {{
            let old = $old;
            let new = $new;
            if !optional_text_equal(&old, &new) {
                result.push(FieldChange {
                    field: $name.to_owned(),
                    old_value: old,
                    new_value: new,
                    significance: $sig,
                    absolute_delta: None,
                    percent_delta: None,
                });
            }
        }};
    }
    macro_rules! decimal_field {
        ($name:literal, $old:expr, $new:expr, $sig:expr) => {{
            if $old != $new {
                let (absolute_delta, percent_delta) = decimal_delta($old, $new);
                result.push(FieldChange {
                    field: $name.to_owned(),
                    old_value: decimal_string($old),
                    new_value: decimal_string($new),
                    significance: $sig,
                    absolute_delta,
                    percent_delta,
                });
            }
        }};
    }
    text_field!(
        "short_text",
        a.short_text.clone(),
        b.short_text.clone(),
        Significance::Medium
    );
    decimal_field!("qty", a.quantity, b.quantity, Significance::Critical);
    text_field!(
        "unit",
        a.unit.clone().unwrap_or_default(),
        b.unit.clone().unwrap_or_default(),
        Significance::High
    );
    decimal_field!(
        "unit_price",
        a.unit_price,
        b.unit_price,
        Significance::Critical
    );
    decimal_field!(
        "total_price",
        a.total_price,
        b.total_price,
        Significance::Critical
    );
    text_field!(
        "item_type",
        item_type_string(&a.item_type),
        item_type_string(&b.item_type),
        Significance::High
    );
    decimal_field!(
        "discount_pct",
        a.discount_pct,
        b.discount_pct,
        Significance::High
    );
    decimal_field!("vat", a.vat, b.vat, Significance::Medium);
    text_field!(
        "markup_type",
        a.markup_type.clone().unwrap_or_default(),
        b.markup_type.clone().unwrap_or_default(),
        Significance::Medium
    );
    text_field!(
        "bim_guid",
        a.bim_guid.clone().unwrap_or_default(),
        b.bim_guid.clone().unwrap_or_default(),
        Significance::Low
    );
    text_field!(
        "change_order_number",
        a.change_order_number.clone().unwrap_or_default(),
        b.change_order_number.clone().unwrap_or_default(),
        Significance::Low
    );
    result
}

fn optional_text_equal(a: &str, b: &str) -> bool {
    a == b
}
fn item_type_string(value: &ItemType) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
fn decimal_string(value: Option<Decimal>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}
fn decimal_delta(old: Option<Decimal>, new: Option<Decimal>) -> (Option<Decimal>, Option<f64>) {
    match (old, new) {
        (Some(old), Some(new)) => {
            let delta = new - old;
            let percent = (old != Decimal::ZERO).then(|| {
                (delta / old * Decimal::ONE_HUNDRED)
                    .to_string()
                    .parse()
                    .unwrap_or(0.0)
            });
            (Some(delta), percent)
        }
        _ => (None, None),
    }
}
