use std::collections::BTreeMap;

use rust_decimal::{Decimal, RoundingStrategy};

use crate::{
    Attachment, Boq, BoqCategory, Document, DocumentMeta, ExchangePhase, GaebError, Item, ItemType,
    Lot, ProcurementDocument, ProjectInfo, RichText, SourceVersion,
};

/// Fluent builder for procurement BoQs.
///
/// The builder creates explicit [`Lot`] values. `add_category` is a convenient
/// single-lot shortcut and creates synthetic lot `1` on first use.
#[derive(Clone, Debug)]
pub struct BoqBuilder {
    phase: ExchangePhase,
    version: SourceVersion,
    project: ProjectInfo,
    lots: Vec<LotBuilder>,
    implicit_lot: Option<LotBuilder>,
}

impl Default for BoqBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl BoqBuilder {
    pub fn new() -> Self {
        Self::with_profile(ExchangePhase::X83, SourceVersion::DaXml33)
    }

    pub fn with_profile(phase: ExchangePhase, version: SourceVersion) -> Self {
        Self {
            phase,
            version,
            project: ProjectInfo::default(),
            lots: Vec::new(),
            implicit_lot: None,
        }
    }

    pub fn phase(&mut self, phase: ExchangePhase) -> &mut Self {
        self.phase = phase;
        self
    }

    pub fn version(&mut self, version: SourceVersion) -> &mut Self {
        self.version = version;
        self
    }

    pub fn project(&mut self, project: ProjectInfo) -> &mut Self {
        self.project = project;
        self
    }

    pub fn project_id(&mut self, id: impl Into<String>) -> &mut Self {
        self.project.id = Some(id.into());
        self
    }

    pub fn project_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.project.name = Some(name.into());
        self
    }

    pub fn project_label(&mut self, label: impl Into<String>) -> &mut Self {
        self.project.label = Some(label.into());
        self
    }

    pub fn currency(&mut self, currency: impl Into<String>) -> &mut Self {
        self.project.currency = Some(currency.into());
        self
    }

    pub fn currency_label(&mut self, label: impl Into<String>) -> &mut Self {
        self.project.currency_label = Some(label.into());
        self
    }

    pub fn boq_id(&mut self, id: impl Into<String>) -> &mut Self {
        self.project.boq_id = Some(id.into());
        self
    }

    pub fn add_lot(
        &mut self,
        rno: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<&mut LotBuilder, GaebError> {
        if self.implicit_lot.is_some() {
            return Err(builder_error(
                "cannot add an explicit lot after using add_category",
            ));
        }
        self.lots.push(LotBuilder::new(rno, label));
        Ok(self.lots.last_mut().expect("lot was pushed"))
    }

    pub fn add_category(
        &mut self,
        rno: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<&mut CategoryBuilder, GaebError> {
        if !self.lots.is_empty() {
            return Err(builder_error(
                "cannot use add_category after add_lot; add categories to a lot",
            ));
        }
        if self.implicit_lot.is_none() {
            self.implicit_lot = Some(LotBuilder::synthetic());
        }
        Ok(self
            .implicit_lot
            .as_mut()
            .expect("implicit lot was created")
            .add_category(rno, label))
    }

    /// Build a lenient procurement document. Compatibility issues become
    /// warnings in `DocumentMeta::warnings`.
    pub fn build(&self) -> Result<Document, GaebError> {
        self.build_with_options(false)
    }

    /// Build a document and reject duplicate OZs and compatibility issues.
    pub fn build_strict(&self) -> Result<Document, GaebError> {
        self.build_with_options(true)
    }

    pub fn build_with_options(&self, strict: bool) -> Result<Document, GaebError> {
        let mut lots = if let Some(lot) = &self.implicit_lot {
            vec![lot.build()]
        } else {
            self.lots.iter().map(LotBuilder::build).collect()
        };
        let mut warnings = Vec::new();

        for lot in &mut lots {
            validate_duplicates(lot, strict, &mut warnings)?;
            for item in lot.items() {
                validate_phase_item(self.phase, item, strict, &mut warnings)?;
                validate_version_item(self.version, self.phase, item, strict, &mut warnings)?;
            }
        }

        let total = lots
            .iter()
            .flat_map(Lot::items)
            .filter(|item| item.item_type.affects_total())
            .filter_map(|item| item.total_price)
            .sum();
        let boq = Boq {
            lots,
            totals: Some(total),
            ..Boq::default()
        };
        let meta = DocumentMeta {
            version: self.version,
            phase: self.phase,
            program: Some("gaeb-core-builder".to_owned()),
            warnings,
            ..DocumentMeta::default()
        };
        Ok(Document::Procurement(ProcurementDocument {
            meta,
            project: self.project.clone(),
            boq,
            invoice: None,
        }))
    }
}

#[derive(Clone, Debug)]
pub struct LotBuilder {
    id: Option<String>,
    rno: String,
    label: String,
    synthetic: bool,
    categories: Vec<CategoryBuilder>,
    remarks: Vec<String>,
    performance_descriptions: Vec<String>,
    declared_total: Option<Decimal>,
}

impl LotBuilder {
    fn new(rno: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: None,
            rno: rno.into(),
            label: label.into(),
            synthetic: false,
            categories: Vec::new(),
            remarks: Vec::new(),
            performance_descriptions: Vec::new(),
            declared_total: None,
        }
    }

    fn synthetic() -> Self {
        let mut lot = Self::new("1", "");
        lot.synthetic = true;
        lot
    }

    pub fn id(&mut self, id: impl Into<String>) -> &mut Self {
        self.id = Some(id.into());
        self
    }

    pub fn remark(&mut self, text: impl Into<String>) -> &mut Self {
        self.remarks.push(text.into());
        self
    }

    pub fn performance_description(&mut self, text: impl Into<String>) -> &mut Self {
        self.performance_descriptions.push(text.into());
        self
    }

    pub fn declared_total(&mut self, total: Decimal) -> &mut Self {
        self.declared_total = Some(total);
        self
    }

    pub fn add_category(
        &mut self,
        rno: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut CategoryBuilder {
        self.categories.push(CategoryBuilder::new(rno, label));
        self.categories.last_mut().expect("category was pushed")
    }

    fn build(&self) -> Lot {
        let categories: Vec<_> = self.categories.iter().map(CategoryBuilder::build).collect();
        let total = categories.iter().map(category_total).sum();
        Lot {
            id: self.id.clone(),
            rno: self.rno.clone(),
            label: self.label.clone(),
            synthetic: self.synthetic,
            categories,
            remarks: self.remarks.clone(),
            performance_descriptions: self.performance_descriptions.clone(),
            declared_total: self.declared_total.or(Some(total)),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CategoryBuilder {
    id: Option<String>,
    rno: String,
    label: String,
    declared_total: Option<Decimal>,
    items: Vec<ItemBuilder>,
    categories: Vec<CategoryBuilder>,
    remarks: Vec<String>,
    performance_descriptions: Vec<String>,
    bid_up_down_request: Option<String>,
    next_sequence: usize,
}

impl CategoryBuilder {
    fn new(rno: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: None,
            rno: rno.into(),
            label: label.into(),
            declared_total: None,
            items: Vec::new(),
            categories: Vec::new(),
            remarks: Vec::new(),
            performance_descriptions: Vec::new(),
            bid_up_down_request: None,
            next_sequence: 0,
        }
    }

    pub fn id(&mut self, id: impl Into<String>) -> &mut Self {
        self.id = Some(id.into());
        self
    }

    pub fn remark(&mut self, text: impl Into<String>) -> &mut Self {
        self.remarks.push(text.into());
        self
    }

    pub fn performance_description(&mut self, text: impl Into<String>) -> &mut Self {
        self.performance_descriptions.push(text.into());
        self
    }

    pub fn bid_up_down_request(&mut self, value: impl Into<String>) -> &mut Self {
        self.bid_up_down_request = Some(value.into());
        self
    }

    pub fn declared_total(&mut self, total: Decimal) -> &mut Self {
        self.declared_total = Some(total);
        self
    }

    pub fn add_subcategory(
        &mut self,
        rno: impl Into<String>,
        label: impl Into<String>,
    ) -> &mut CategoryBuilder {
        self.categories.push(CategoryBuilder::new(rno, label));
        self.categories.last_mut().expect("subcategory was pushed")
    }

    pub fn add_item(&mut self, short_text: impl Into<String>) -> &mut ItemBuilder {
        self.add_item_inner(None, short_text)
    }

    pub fn add_item_with_oz(
        &mut self,
        oz: impl Into<String>,
        short_text: impl Into<String>,
    ) -> &mut ItemBuilder {
        self.add_item_inner(Some(oz.into()), short_text)
    }

    fn add_item_inner(
        &mut self,
        oz: Option<String>,
        short_text: impl Into<String>,
    ) -> &mut ItemBuilder {
        self.next_sequence += 1;
        let oz = oz.unwrap_or_else(|| format!("{}.{:04}", self.rno, self.next_sequence * 10));
        self.items.push(ItemBuilder::new(oz, short_text));
        self.items.last_mut().expect("item was pushed")
    }

    fn build(&self) -> BoqCategory {
        let categories: Vec<_> = self.categories.iter().map(CategoryBuilder::build).collect();
        let items: Vec<_> = self.items.iter().map(ItemBuilder::build).collect();
        let total = items
            .iter()
            .filter(|item| item.item_type.affects_total())
            .filter_map(|item| item.total_price)
            .sum::<Decimal>()
            + categories.iter().map(category_total).sum::<Decimal>();
        BoqCategory {
            id: self.id.clone(),
            rno: self.rno.clone(),
            label: self.label.clone(),
            declared_total: self.declared_total.or(Some(total)),
            items,
            remarks: self.remarks.clone(),
            performance_descriptions: self.performance_descriptions.clone(),
            bid_up_down_request: self.bid_up_down_request.clone(),
            categories,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ItemBuilder {
    item: Item,
}

impl ItemBuilder {
    fn new(oz: String, short_text: impl Into<String>) -> Self {
        Self {
            item: Item {
                oz,
                short_text: short_text.into(),
                ..Item::default()
            },
        }
    }

    pub fn item_mut(&mut self) -> &mut Item {
        &mut self.item
    }

    pub fn quantity(&mut self, quantity: Decimal) -> &mut Self {
        self.item.quantity = Some(quantity);
        self
    }

    pub fn unit(&mut self, unit: impl Into<String>) -> &mut Self {
        self.item.unit = Some(unit.into());
        self
    }

    pub fn unit_price(&mut self, price: Decimal) -> &mut Self {
        self.item.unit_price = Some(price);
        self
    }

    pub fn total_price(&mut self, price: Decimal) -> &mut Self {
        self.item.total_price = Some(price);
        self
    }

    pub fn item_type(&mut self, item_type: ItemType) -> &mut Self {
        self.item.item_type = item_type;
        self
    }

    pub fn bim_guid(&mut self, guid: impl Into<String>) -> &mut Self {
        self.item.bim_guid = Some(guid.into());
        self
    }

    pub fn change_order_number(&mut self, number: impl Into<String>) -> &mut Self {
        self.item.change_order_number = Some(number.into());
        self
    }

    pub fn set_long_text(&mut self, text: impl Into<String>) -> &mut Self {
        let text = text.into();
        self.item.long_text = text.clone();
        self.item.rich_text = Some(RichText {
            paragraphs: vec![text.clone()],
            plain_text: text,
            ..RichText::default()
        });
        self
    }

    pub fn add_attachment(
        &mut self,
        filename: impl Into<String>,
        data: Vec<u8>,
        mime_type: impl Into<String>,
    ) -> &mut Self {
        self.item.attachments.push(Attachment {
            filename: filename.into(),
            mime_type: mime_type.into(),
            size_bytes: data.len(),
            data,
        });
        self
    }

    fn build(&self) -> Item {
        let mut item = self.item.clone();
        if item.total_price.is_none()
            && let (Some(quantity), Some(unit_price)) = (item.quantity, item.unit_price)
        {
            item.total_price = Some(
                (quantity * unit_price)
                    .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero),
            );
        }
        item
    }
}

fn category_total(category: &BoqCategory) -> Decimal {
    category
        .items
        .iter()
        .filter(|item| item.item_type.affects_total())
        .filter_map(|item| item.total_price)
        .sum::<Decimal>()
        + category
            .categories
            .iter()
            .map(category_total)
            .sum::<Decimal>()
}

fn validate_duplicates(
    lot: &Lot,
    strict: bool,
    warnings: &mut Vec<String>,
) -> Result<(), GaebError> {
    let mut seen = BTreeMap::new();
    for item in lot.items() {
        *seen.entry(item.full_oz()).or_insert(0usize) += 1;
    }
    let duplicates: Vec<_> = seen.into_iter().filter(|(_, count)| *count > 1).collect();
    if duplicates.is_empty() {
        return Ok(());
    }
    let message = format!(
        "Lot {:?}: duplicate OZ numbers: {}",
        lot.rno,
        duplicates
            .iter()
            .map(|(oz, count)| format!("{oz:?} ({count}x)"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    report(message, strict, warnings)
}

fn validate_phase_item(
    phase: ExchangePhase,
    item: &Item,
    strict: bool,
    warnings: &mut Vec<String>,
) -> Result<(), GaebError> {
    if item.item_type != ItemType::Normal {
        return Ok(());
    }
    let label = match phase {
        ExchangePhase::X80 => Some((
            "blank BoQ",
            &["unit_price", "total_price"] as &[&str],
            &[][..],
        )),
        ExchangePhase::X82 => Some(("cost estimate/Preisspiegel", &[][..], &["unit_price"][..])),
        ExchangePhase::X83 => Some(("tender with prices", &[][..], &["unit_price"][..])),
        ExchangePhase::X84 => Some(("award", &[][..], &["unit_price"][..])),
        ExchangePhase::X88 => Some((
            "addendum/Nachtrag",
            &[][..],
            &["unit_price", "change_order_number"][..],
        )),
        _ => None,
    };
    let Some((label, warn_present, warn_missing)) = label else {
        return Ok(());
    };
    for field in warn_present {
        let present = match *field {
            "unit_price" => item.unit_price.is_some(),
            "total_price" => item.total_price.is_some(),
            _ => false,
        };
        if present {
            report(
                format!(
                    "Item {}: {field:?} is set but phase {} ({label}) typically should not have it.",
                    item.full_oz(),
                    phase.code()
                ),
                strict,
                warnings,
            )?;
        }
    }
    for field in warn_missing {
        let missing = match *field {
            "unit_price" => item.unit_price.is_none(),
            "change_order_number" => item.change_order_number.is_none(),
            _ => false,
        };
        if missing {
            report(
                format!(
                    "Item {}: {field:?} is missing for phase {} ({label}).",
                    item.full_oz(),
                    phase.code()
                ),
                strict,
                warnings,
            )?;
        }
    }
    Ok(())
}

fn validate_version_item(
    version: SourceVersion,
    phase: ExchangePhase,
    item: &Item,
    strict: bool,
    warnings: &mut Vec<String>,
) -> Result<(), GaebError> {
    let check_min = |field: &str, present: bool, minimum: SourceVersion| {
        if present && version_rank(version) < version_rank(minimum) {
            Some(format!(
                "Item {}: {field:?} requires DA XML {}+, but target is {}. This field will be dropped during export.",
                item.full_oz(),
                minimum.number(),
                version.number()
            ))
        } else {
            None
        }
    };
    for (field, present, minimum) in [
        ("bim_guid", item.bim_guid.is_some(), SourceVersion::DaXml33),
        (
            "attachments",
            !item.attachments.is_empty(),
            SourceVersion::DaXml33,
        ),
        (
            "change_order_number",
            item.change_order_number.is_some(),
            SourceVersion::DaXml31,
        ),
        (
            "up_components",
            !item.up_components.is_empty(),
            SourceVersion::DaXml30,
        ),
        (
            "discount_pct",
            item.discount_pct.is_some(),
            SourceVersion::DaXml30,
        ),
        (
            "ctlg_assigns",
            !item.ctlg_assigns.is_empty(),
            SourceVersion::DaXml31,
        ),
    ] {
        if let Some(message) = check_min(field, present, minimum) {
            report(message, strict, warnings)?;
        }
    }
    for (field, present, allowed) in [
        (
            "cost_approaches",
            !item.cost_approaches.is_empty(),
            &[ExchangePhase::X50, ExchangePhase::X51, ExchangePhase::X52][..],
        ),
        (
            "markup_type",
            item.markup_type.is_some(),
            &[ExchangePhase::X52][..],
        ),
        (
            "markup_sub_qtys",
            !item.markup_sub_qtys.is_empty(),
            &[ExchangePhase::X52][..],
        ),
    ] {
        if present && !allowed.contains(&phase) {
            report(
                format!(
                    "Item {}: {field:?} is only valid for phases {}, but target is {}.",
                    item.full_oz(),
                    allowed
                        .iter()
                        .map(|value| value.code())
                        .collect::<Vec<_>>()
                        .join(", "),
                    phase.code()
                ),
                strict,
                warnings,
            )?;
        }
    }
    Ok(())
}

fn version_rank(version: SourceVersion) -> u8 {
    match version {
        SourceVersion::DaXml20 => 0,
        SourceVersion::DaXml21 => 1,
        SourceVersion::DaXml30 => 2,
        SourceVersion::DaXml31 => 3,
        SourceVersion::DaXml32 => 4,
        SourceVersion::DaXml33 => 5,
        SourceVersion::Gaeb90 => 0,
    }
}

fn report(message: String, strict: bool, warnings: &mut Vec<String>) -> Result<(), GaebError> {
    if strict {
        Err(GaebError::Validation(message))
    } else {
        warnings.push(message);
        Ok(())
    }
}

fn builder_error(message: &str) -> GaebError {
    GaebError::Validation(format!("BoQ builder: {message}"))
}
