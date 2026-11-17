use std::collections::BTreeMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{GaebError, ValidationIssue, WriteOptions, WriteReport};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceVersion {
    DaXml20,
    DaXml21,
    DaXml30,
    DaXml31,
    DaXml32,
    #[default]
    DaXml33,
    Gaeb90,
}

impl SourceVersion {
    pub const fn number(self) -> &'static str {
        match self {
            Self::DaXml20 => "2.0",
            Self::DaXml21 => "2.1",
            Self::DaXml30 => "3.0",
            Self::DaXml31 => "3.1",
            Self::DaXml32 => "3.2",
            Self::DaXml33 => "3.3",
            Self::Gaeb90 => "90",
        }
    }

    pub(crate) fn from_number(value: &str) -> Option<Self> {
        Some(match value.trim() {
            "2.0" => Self::DaXml20,
            "2.1" => Self::DaXml21,
            "3.0" => Self::DaXml30,
            "3.1" => Self::DaXml31,
            "3.2" => Self::DaXml32,
            "3.3" => Self::DaXml33,
            "90" => Self::Gaeb90,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ExchangePhase {
    X31,
    X32,
    X33,
    X34,
    X35,
    X36,
    X37,
    X38,
    X39,
    X50,
    X51,
    X52,
    X60,
    X61,
    X62,
    X63,
    X64,
    X65,
    X66,
    X67,
    X68,
    X69,
    X80,
    X81,
    X82,
    #[default]
    X83,
    X84,
    X85,
    X86,
    X87,
    /// Historical phase. It is invalid with DA XML 3.3.
    X88,
    X89,
    X89B,
    X83Z,
    X84Z,
    X86ZR,
    X86ZE,
    X93,
    X94,
    /// Retained for compatibility, but not part of a released GAEB phase set.
    X95,
    X96,
    X97,
}

impl ExchangePhase {
    pub const fn code(self) -> &'static str {
        match self {
            Self::X31 => "X31",
            Self::X32 => "X32",
            Self::X33 => "X33",
            Self::X34 => "X34",
            Self::X35 => "X35",
            Self::X36 => "X36",
            Self::X37 => "X37",
            Self::X38 => "X38",
            Self::X39 => "X39",
            Self::X50 => "X50",
            Self::X51 => "X51",
            Self::X52 => "X52",
            Self::X60 => "X60",
            Self::X61 => "X61",
            Self::X62 => "X62",
            Self::X63 => "X63",
            Self::X64 => "X64",
            Self::X65 => "X65",
            Self::X66 => "X66",
            Self::X67 => "X67",
            Self::X68 => "X68",
            Self::X69 => "X69",
            Self::X80 => "X80",
            Self::X81 => "X81",
            Self::X82 => "X82",
            Self::X83 => "X83",
            Self::X84 => "X84",
            Self::X85 => "X85",
            Self::X86 => "X86",
            Self::X87 => "X87",
            Self::X88 => "X88",
            Self::X89 => "X89",
            Self::X89B => "X89B",
            Self::X83Z => "X83Z",
            Self::X84Z => "X84Z",
            Self::X86ZR => "X86ZR",
            Self::X86ZE => "X86ZE",
            Self::X93 => "X93",
            Self::X94 => "X94",
            Self::X95 => "X95",
            Self::X96 => "X96",
            Self::X97 => "X97",
        }
    }

    pub(crate) fn from_dp(value: &str) -> Option<Self> {
        let value = value.trim();
        let value = if value.len() >= 2 && value[..2].eq_ignore_ascii_case("DA") {
            &value[2..]
        } else {
            value.trim_start_matches(['X', 'x', 'D', 'd'])
        };
        Some(match value.to_ascii_uppercase().as_str() {
            "31" => Self::X31,
            "32" => Self::X32,
            "33" => Self::X33,
            "34" => Self::X34,
            "35" => Self::X35,
            "36" => Self::X36,
            "37" => Self::X37,
            "38" => Self::X38,
            "39" => Self::X39,
            "50" | "50.1" | "50.2" => Self::X50,
            "51" | "51.1" | "51.2" => Self::X51,
            "52" => Self::X52,
            "60" => Self::X60,
            "61" => Self::X61,
            "62" => Self::X62,
            "63" => Self::X63,
            "64" => Self::X64,
            "65" => Self::X65,
            "66" => Self::X66,
            "67" => Self::X67,
            "68" => Self::X68,
            "69" => Self::X69,
            "80" => Self::X80,
            "81" => Self::X81,
            "82" => Self::X82,
            "83" => Self::X83,
            "84" => Self::X84,
            "85" => Self::X85,
            "86" => Self::X86,
            "87" => Self::X87,
            "88" => Self::X88,
            "89" => Self::X89,
            "89B" => Self::X89B,
            "83Z" => Self::X83Z,
            "84Z" => Self::X84Z,
            "86ZR" => Self::X86ZR,
            "86ZE" => Self::X86ZE,
            "93" => Self::X93,
            "94" => Self::X94,
            "95" => Self::X95,
            "96" => Self::X96,
            "97" => Self::X97,
            _ => return None,
        })
    }

    pub const fn kind(self) -> DocumentKind {
        match self {
            Self::X31 => DocumentKind::Quantity,
            Self::X32
            | Self::X33
            | Self::X34
            | Self::X35
            | Self::X36
            | Self::X37
            | Self::X38
            | Self::X39
            | Self::X60
            | Self::X61
            | Self::X62
            | Self::X63
            | Self::X64
            | Self::X65
            | Self::X66
            | Self::X67
            | Self::X68
            | Self::X69 => DocumentKind::Structured,
            Self::X50 | Self::X51 => DocumentKind::Cost,
            Self::X93 | Self::X94 | Self::X95 | Self::X96 | Self::X97 => DocumentKind::Trade,
            _ => DocumentKind::Procurement,
        }
    }

    pub const fn is_current_da_xml_33(self) -> bool {
        !matches!(self, Self::X88 | Self::X95)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentKind {
    #[default]
    Procurement,
    Trade,
    Cost,
    Quantity,
    Structured,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DocumentMeta {
    pub version: SourceVersion,
    pub phase: ExchangePhase,
    pub namespace: Option<String>,
    pub schema_release: Option<String>,
    pub date: Option<String>,
    pub time: Option<String>,
    pub program: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProjectInfo {
    pub id: Option<String>,
    pub name: Option<String>,
    pub label: Option<String>,
    pub currency: Option<String>,
    pub currency_label: Option<String>,
    pub boq_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceAddress {
    pub name1: String,
    pub name2: Option<String>,
    pub name3: Option<String>,
    pub name4: Option<String>,
    pub street: String,
    pub postal_code: String,
    pub city: String,
    pub country: Option<String>,
    pub iln: Option<String>,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
    pub email: Option<String>,
    pub vat_id: Option<String>,
    pub banks: Vec<InvoiceBank>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceBank {
    pub label: String,
    pub bank_code: String,
    pub account_number: String,
    pub bic: Option<String>,
    pub iban: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceParty {
    pub address: Option<InvoiceAddress>,
    pub tax_no: Option<String>,
    pub dp_no: Option<String>,
    pub award_no: Option<String>,
    pub account_receivable_no: Option<String>,
    pub accounts_payable_no: Option<String>,
    pub bidder_no: Option<String>,
    pub request_id_no: Option<String>,
    pub request_name: Option<String>,
    pub country_type: Option<String>,
    pub country_name: Option<String>,
    pub sector_type: Option<String>,
    pub preferred_bidder_type: Option<String>,
    pub service_type: Option<String>,
    pub additional_texts: Vec<InvoiceText>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceSite {
    pub address: Option<InvoiceAddress>,
    pub id_no: Option<String>,
    pub name: Option<String>,
    pub additional_texts: Vec<InvoiceText>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceText {
    pub outline: Option<String>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceHeader {
    pub invoice_no: Option<String>,
    pub reference_invoice_no: Option<String>,
    pub invoice_date: Option<String>,
    pub invoice_type: Option<String>,
    pub credit_note: bool,
    pub settlement_type: Option<String>,
    pub sequential_no: Option<i64>,
    pub service_provision_start_date: String,
    pub service_provision_end_date: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceShare {
    pub share_type: String,
    pub description: String,
    pub total: Option<Decimal>,
    pub percent: Option<Decimal>,
    pub counter_claim: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoicePaymentTerm {
    pub cash_discount_days: Option<u32>,
    pub days_type: Option<String>,
    pub cash_discount_date: Option<String>,
    pub discount_pct: Decimal,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoicePayment {
    pub total_vat: Decimal,
    pub discount_amount: Option<Decimal>,
    pub discount_vat_amount: Option<Decimal>,
    pub total: Decimal,
    pub payment_date: String,
    pub invoice_type: Option<String>,
    pub invoice_no: String,
    pub payment_no: Option<String>,
    pub payment_note: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceIssued {
    pub total_vat: Decimal,
    pub total: Decimal,
    pub invoice_date: String,
    pub invoice_type: String,
    pub invoice_no: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceCheckNote {
    pub state: String,
    pub check_date: String,
    pub annotation: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct InvoiceDocument {
    pub owner: Option<InvoiceParty>,
    pub requesters: Vec<InvoiceParty>,
    pub contractor: Option<InvoiceParty>,
    pub construction_sites: Vec<InvoiceSite>,
    pub notification_sites: Vec<InvoiceSite>,
    pub additional_texts: Vec<InvoiceText>,
    pub header: Option<InvoiceHeader>,
    pub creator: Option<InvoiceParty>,
    pub recipient: Option<InvoiceParty>,
    pub check_notes: Vec<InvoiceCheckNote>,
    pub budget_department: Option<String>,
    pub booking_number: Option<String>,
    pub quantity_determination_remark: Option<String>,
    pub shares: Vec<InvoiceShare>,
    pub payment_terms: Vec<InvoicePaymentTerm>,
    pub payments_made: Vec<InvoicePayment>,
    pub invoices_made_out: Vec<InvoiceIssued>,
    pub total_gross: Option<Decimal>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostType {
    pub name: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Boq {
    pub id: Option<String>,
    pub name: Option<String>,
    pub cost_types: Vec<CostType>,
    /// Legacy single-lot representation kept for source compatibility.
    pub categories: Vec<BoqCategory>,
    /// Explicit lots for multi-lot BoQs.
    pub lots: Vec<Lot>,
    pub remarks: Vec<String>,
    pub performance_descriptions: Vec<String>,
    pub totals: Option<Decimal>,
}

impl Boq {
    pub fn items(&self) -> Box<dyn Iterator<Item = &Item> + '_> {
        if self.lots.is_empty() {
            Box::new(self.categories.iter().flat_map(BoqCategory::items))
        } else {
            Box::new(self.lots.iter().flat_map(Lot::items))
        }
    }

    pub fn items_mut(&mut self) -> Box<dyn Iterator<Item = &mut Item> + '_> {
        if self.lots.is_empty() {
            Box::new(self.categories.iter_mut().flat_map(BoqCategory::items_mut))
        } else {
            Box::new(self.lots.iter_mut().flat_map(Lot::items_mut))
        }
    }

    pub fn top_level_categories(&self) -> Box<dyn Iterator<Item = &BoqCategory> + '_> {
        if self.lots.is_empty() {
            Box::new(self.categories.iter())
        } else {
            Box::new(self.lots.iter().flat_map(|lot| lot.categories.iter()))
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Lot {
    pub id: Option<String>,
    pub rno: String,
    pub label: String,
    pub synthetic: bool,
    pub categories: Vec<BoqCategory>,
    pub remarks: Vec<String>,
    pub performance_descriptions: Vec<String>,
    pub declared_total: Option<Decimal>,
}

impl Lot {
    pub fn items(&self) -> Box<dyn Iterator<Item = &Item> + '_> {
        Box::new(self.categories.iter().flat_map(BoqCategory::items))
    }

    pub fn items_mut(&mut self) -> Box<dyn Iterator<Item = &mut Item> + '_> {
        Box::new(self.categories.iter_mut().flat_map(BoqCategory::items_mut))
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BoqCategory {
    pub id: Option<String>,
    pub rno: String,
    pub label: String,
    pub declared_total: Option<Decimal>,
    pub items: Vec<Item>,
    pub remarks: Vec<String>,
    pub performance_descriptions: Vec<String>,
    pub bid_up_down_request: Option<String>,
    pub categories: Vec<Self>,
}

impl BoqCategory {
    pub fn items(&self) -> Box<dyn Iterator<Item = &Item> + '_> {
        Box::new(
            self.items
                .iter()
                .chain(self.categories.iter().flat_map(Self::items)),
        )
    }

    pub fn items_mut(&mut self) -> Box<dyn Iterator<Item = &mut Item> + '_> {
        Box::new(
            self.items
                .iter_mut()
                .chain(self.categories.iter_mut().flat_map(Self::items_mut)),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemType {
    #[default]
    Normal,
    LumpSum,
    Alternative,
    Eventual,
    TextOnly,
    Supplement,
    Markup,
}

impl ItemType {
    pub const fn affects_total(&self) -> bool {
        matches!(self, Self::Normal | Self::LumpSum | Self::Supplement)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BidderPrice {
    pub bidder_name: String,
    pub bidder_id: Option<String>,
    pub unit_price: Option<Decimal>,
    pub total_price: Option<Decimal>,
    pub affects_total: bool,
    pub rank: Option<usize>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RichText {
    pub paragraphs: Vec<String>,
    pub tables: Vec<Vec<Vec<String>>>,
    pub images: Vec<String>,
    pub raw_html: Option<String>,
    pub plain_text: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
    pub size_bytes: usize,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ClassificationResult {
    pub trade: String,
    pub element_type: String,
    pub sub_type: String,
    pub confidence: Decimal,
    pub flag: String,
    pub ifc_type: Option<String>,
    pub din276_code: Option<String>,
    pub cached: bool,
    pub prompt_version: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ExtractionResult {
    pub schema_name: String,
    pub schema_hash: String,
    pub data: serde_json::Value,
    pub completeness: Decimal,
    pub cached: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostApproach {
    pub cost_type: String,
    pub amount: Option<Decimal>,
    pub remark: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QtySplit {
    pub label: String,
    pub quantity: Decimal,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MarkupSubQty {
    pub ref_rno: String,
    pub ref_id: Option<String>,
    pub quantity: Option<Decimal>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CtlgAssign {
    pub ctlg_id: String,
    pub ctlg_code: String,
    pub quantity: Option<Decimal>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TextComplement {
    pub kind: String,
    pub mark_label: Option<String>,
    pub empty: bool,
    pub caption: Option<String>,
    pub text: String,
    pub tail: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SubDescription {
    pub number: String,
    pub short_text: String,
    pub long_text: String,
    pub text_complements: Vec<TextComplement>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StandardService {
    pub catalog: Option<String>,
    pub catalog_version: Option<String>,
    pub work_category: Option<String>,
    pub group: Option<String>,
    pub identifiers: Vec<String>,
    pub identifier_indices: Vec<String>,
    pub change_identifiers: Vec<String>,
    pub number: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Item {
    pub id: Option<String>,
    pub oz: String,
    pub oz_path: Vec<String>,
    pub rno_index: Option<String>,
    pub alternative_group: Option<String>,
    pub alternative_serial: Option<String>,
    pub short_text: String,
    pub long_text: String,
    pub rich_text: Option<RichText>,
    pub attachments: Vec<Attachment>,
    pub classification: Option<ClassificationResult>,
    pub extractions: BTreeMap<String, ExtractionResult>,
    pub raw_data: BTreeMap<String, String>,
    pub quantity: Option<Decimal>,
    pub predicted_quantity: Option<Decimal>,
    pub bill_quantity: Option<Decimal>,
    pub quantity_tbd: bool,
    pub unit: Option<String>,
    pub unit_price: Option<Decimal>,
    pub total_price: Option<Decimal>,
    pub item_type: ItemType,
    pub source_item_kind: Option<String>,
    pub vat: Option<Decimal>,
    pub bim_guid: Option<String>,
    pub change_order_number: Option<String>,
    pub co_status: Option<String>,
    pub discount_pct: Option<Decimal>,
    pub markup_type: Option<String>,
    pub not_applicable: bool,
    pub hour_item: bool,
    pub markup_item: bool,
    pub up_breakdown: bool,
    pub sum_description: bool,
    pub bid_up_down_request: Option<String>,
    pub bid_up_down_pct: Option<Decimal>,
    pub reference_description: Option<String>,
    pub reference_rno: Option<String>,
    pub reference_performance: Option<String>,
    pub up_from: Option<Decimal>,
    pub up_avg: Option<Decimal>,
    pub up_to: Option<Decimal>,
    pub up_components: Vec<Decimal>,
    pub qty_splits: Vec<QtySplit>,
    pub bidder_prices: Vec<BidderPrice>,
    pub cost_approaches: Vec<CostApproach>,
    pub markup_sub_qtys: Vec<MarkupSubQty>,
    pub ctlg_assigns: Vec<CtlgAssign>,
    pub text_complements: Vec<TextComplement>,
    pub sub_descriptions: Vec<SubDescription>,
    pub standard_service: Option<StandardService>,
    pub catalog_number: Option<String>,
    pub extensions: BTreeMap<String, String>,
}

impl Item {
    pub fn full_oz(&self) -> String {
        if self.oz_path.is_empty() {
            self.oz.clone()
        } else {
            [self.oz_path.as_slice(), std::slice::from_ref(&self.oz)]
                .concat()
                .join(".")
        }
    }

    pub fn computed_total(&self) -> Option<Decimal> {
        self.quantity.zip(self.unit_price).map(|(q, p)| q * p)
    }

    pub fn long_text_plain(&self) -> &str {
        self.rich_text
            .as_ref()
            .map(|text| text.plain_text.as_str())
            .filter(|text| !text.is_empty())
            .unwrap_or(&self.long_text)
    }

    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    pub fn has_rounding_discrepancy(&self) -> bool {
        self.total_price
            .zip(self.computed_total())
            .is_some_and(|(actual, computed)| (actual - computed).abs() > Decimal::new(1, 2))
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ProcurementDocument {
    pub meta: DocumentMeta,
    pub project: ProjectInfo,
    pub boq: Boq,
    pub invoice: Option<InvoiceDocument>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TradeDocument {
    pub meta: DocumentMeta,
    pub project: ProjectInfo,
    pub order_number: Option<String>,
    pub order_reference: Option<String>,
    pub currency: Option<String>,
    pub order_date: Option<String>,
    pub delivery_date: Option<String>,
    pub supplier: Option<TradeParty>,
    pub customer: Option<TradeParty>,
    pub delivery_place: Option<TradeParty>,
    pub planner: Option<TradeParty>,
    pub invoice_recipient: Option<TradeParty>,
    pub catalog_assigns: Vec<CtlgAssign>,
    pub items: Vec<TradeItem>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TradeParty {
    pub address: Option<TradeAddress>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TradeAddress {
    pub name: Option<String>,
    pub name2: Option<String>,
    pub name3: Option<String>,
    pub name4: Option<String>,
    pub street: Option<String>,
    pub postal_code: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub contact: Option<String>,
    pub phone: Option<String>,
    pub fax: Option<String>,
    pub email: Option<String>,
    pub iln: Option<String>,
    pub vat_id: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TradeItem {
    pub id: String,
    pub item_chara: Option<String>,
    pub item_type: Option<String>,
    pub service: Option<String>,
    pub ean: Option<String>,
    pub article_number_id: Option<String>,
    pub article_number: Option<String>,
    pub supplier_article_number_id: Option<String>,
    pub supplier_article_number: Option<String>,
    pub customer_article_number: Option<String>,
    pub catalog_article_number: Option<String>,
    pub catalog_number: Option<String>,
    pub short_text: String,
    pub long_text: String,
    pub quantity: Option<Decimal>,
    pub unit: Option<String>,
    pub offer_price: Option<Decimal>,
    pub net_price: Option<Decimal>,
    pub price_basis: Option<Decimal>,
    pub alternate_unit: Option<String>,
    pub mode_of_shipment: Option<String>,
    pub delivery_date: Option<String>,
    pub delivery_time: Option<String>,
    pub delivery_week: Option<String>,
    pub delivery_chara: Option<String>,
    pub is_service: bool,
    pub ctlg_assigns: Vec<CtlgAssign>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostDocument {
    pub meta: DocumentMeta,
    /// Cost exchange subtype (`50.1`, `50.2`, `51.1`, or `51.2`).
    pub dp: Option<String>,
    pub name: Option<String>,
    pub info: CostInfo,
    pub categories: Vec<CostCategory>,
    pub dimension_elements: Vec<CostDimensionElement>,
    pub category_elements: Vec<CostCategoryElement>,
    pub items: Vec<CostItem>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostBreakdown {
    pub bkdn_type: String,
    pub label: String,
    pub length: Option<usize>,
    pub is_numeric: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostConsortiumMember {
    pub description: String,
    pub address: Option<TradeAddress>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostInfo {
    pub label: Option<String>,
    pub cost_type: Option<String>,
    pub cost_method: Option<String>,
    pub date: Option<String>,
    pub currency: Option<String>,
    pub currency_label: Option<String>,
    pub date_of_price: Option<String>,
    pub date_of_information: Option<String>,
    pub total_net: Option<Decimal>,
    pub total_gross: Option<Decimal>,
    pub breakdowns: Vec<CostBreakdown>,
    pub consortium_members: Vec<CostConsortiumMember>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostProperty {
    pub name: String,
    pub label: String,
    pub arithmetic_quantity_approach: Option<String>,
    pub value_quantity_approach: Option<Decimal>,
    pub unit: String,
    pub property_type: Option<String>,
    pub cad_id: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostReference {
    pub id_ref: String,
    pub reference_kind: String,
    pub reference_type: Option<String>,
    pub portion: Option<Decimal>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostReferenceGroup {
    pub title: String,
    pub references: Vec<CostReference>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostCategory {
    pub id: String,
    pub description: String,
    pub portion: Option<Decimal>,
    pub properties: Vec<CostProperty>,
    pub totals_net: Option<Decimal>,
    pub totals_gross: Option<Decimal>,
    pub categories: Vec<Self>,
    pub items: Vec<CostItem>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostDimensionElement {
    pub id: String,
    pub description: String,
    pub category_id: Option<String>,
    pub remark: String,
    pub properties: Vec<CostProperty>,
    pub quantity: Option<Decimal>,
    pub unit: String,
    pub markup: Option<Decimal>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostCategoryElement {
    pub id: String,
    pub description: String,
    pub category_id: Option<String>,
    pub remark: String,
    pub properties: Vec<CostProperty>,
    pub markup: Option<Decimal>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CostItem {
    pub id: String,
    pub short_text: String,
    pub quantity: Option<Decimal>,
    pub unit: Option<String>,
    pub unit_price: Option<Decimal>,
    pub total_price: Option<Decimal>,
    pub cat_id: Option<String>,
    pub ctlg_assigns: Vec<CtlgAssign>,
    pub remark: String,
    pub portion: Option<Decimal>,
    pub declared_total: Option<Decimal>,
    pub markup: Option<Decimal>,
    pub up_from: Option<Decimal>,
    pub up_avg: Option<Decimal>,
    pub up_to: Option<Decimal>,
    pub is_bill_element: bool,
    pub properties: Vec<CostProperty>,
    pub reference_groups: Vec<CostReferenceGroup>,
    pub children: Vec<Self>,
}

impl CostItem {
    pub fn computed_total(&self) -> Option<Decimal> {
        self.quantity.zip(self.unit_price).map(|(q, p)| q * p)
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityDocument {
    pub meta: DocumentMeta,
    pub reference_project_name: Option<String>,
    pub reference_project_id: Option<String>,
    pub reference_boq_name: Option<String>,
    pub reference_boq_id: Option<String>,
    pub method: Option<String>,
    pub order_description: Option<String>,
    pub project_description: Option<String>,
    pub service_start: Option<String>,
    pub service_end: Option<String>,
    pub owner: Option<TradeAddress>,
    pub contractor: Option<TradeAddress>,
    pub catalogs: Vec<QuantityCatalog>,
    pub attachments: Vec<QuantityAttachment>,
    pub categories: Vec<QuantityCategory>,
    pub items: Vec<QuantityItem>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityCatalog {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub assignment_type: String,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityAttachment {
    pub name: String,
    pub text: String,
    pub description: String,
    pub file_type: String,
    pub data: Vec<u8>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityCategory {
    pub rno: String,
    pub items: Vec<QuantityItem>,
    pub categories: Vec<Self>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityDeterminationItem {
    pub raw: String,
    pub catalog_assigns: Vec<CtlgAssign>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QuantityItem {
    pub oz: String,
    pub rno_part: String,
    pub rno_index: String,
    pub quantity: Option<Decimal>,
    pub determination_items: Vec<QuantityDeterminationItem>,
    pub ctlg_assigns: Vec<CtlgAssign>,
    pub takeoff_rows: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct XmlElement {
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    pub text: String,
    pub children: Vec<Self>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct StructuredDocument {
    pub meta: DocumentMeta,
    /// Phase-specific content retained as a typed XML tree when no common GAEB
    /// item model exists yet. This preserves every element, attribute, and text
    /// value for X32-X39 and X60-X69 round trips.
    pub elements: Vec<XmlElement>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", content = "document", rename_all = "kebab-case")]
#[allow(clippy::large_enum_variant)]
pub enum Document {
    Procurement(ProcurementDocument),
    Trade(TradeDocument),
    Cost(CostDocument),
    Quantity(QuantityDocument),
    Structured(StructuredDocument),
}

impl Document {
    pub const fn kind(&self) -> DocumentKind {
        match self {
            Self::Procurement(_) => DocumentKind::Procurement,
            Self::Trade(_) => DocumentKind::Trade,
            Self::Cost(_) => DocumentKind::Cost,
            Self::Quantity(_) => DocumentKind::Quantity,
            Self::Structured(_) => DocumentKind::Structured,
        }
    }
    pub const fn meta(&self) -> &DocumentMeta {
        match self {
            Self::Procurement(d) => &d.meta,
            Self::Trade(d) => &d.meta,
            Self::Cost(d) => &d.meta,
            Self::Quantity(d) => &d.meta,
            Self::Structured(d) => &d.meta,
        }
    }
    pub const fn source_version(&self) -> SourceVersion {
        self.meta().version
    }
    pub const fn phase(&self) -> ExchangePhase {
        self.meta().phase
    }
    pub fn items(&self) -> impl Iterator<Item = DocumentItem<'_>> {
        let items: Box<dyn Iterator<Item = DocumentItem<'_>> + '_> = match self {
            Self::Procurement(d) => Box::new(d.boq.items().map(DocumentItem::Procurement)),
            Self::Trade(d) => Box::new(d.items.iter().map(DocumentItem::Trade)),
            Self::Cost(d) => Box::new(cost_items(&d.items).map(DocumentItem::Cost)),
            Self::Quantity(d) => Box::new(d.items.iter().map(DocumentItem::Quantity)),
            Self::Structured(_) => Box::new(std::iter::empty()),
        };
        items
    }

    pub fn item_count(&self) -> usize {
        self.items().count()
    }

    pub fn grand_total(&self) -> Decimal {
        match self {
            Self::Procurement(document) => document
                .boq
                .items()
                .filter(|item| item.item_type.affects_total())
                .filter_map(|item| item.total_price)
                .sum(),
            Self::Trade(document) => document
                .items
                .iter()
                .map(|item| {
                    let price = item.net_price.or(item.offer_price).unwrap_or_default();
                    price * item.quantity.unwrap_or(Decimal::ONE)
                })
                .sum(),
            Self::Cost(document) => cost_items(&document.items)
                .filter_map(|item| item.total_price.or_else(|| item.computed_total()))
                .sum(),
            Self::Quantity(_) | Self::Structured(_) => Decimal::ZERO,
        }
    }

    pub fn computed_grand_total(&self) -> Decimal {
        match self {
            Self::Procurement(document) => document
                .boq
                .items()
                .filter(|item| item.item_type.affects_total())
                .filter_map(Item::computed_total)
                .sum(),
            Self::Trade(_) | Self::Quantity(_) | Self::Structured(_) => self.grand_total(),
            Self::Cost(document) => cost_items(&document.items)
                .filter_map(CostItem::computed_total)
                .sum(),
        }
    }

    pub fn write<W: std::io::Write>(
        &self,
        writer: W,
        options: &WriteOptions,
    ) -> Result<WriteReport, GaebError> {
        crate::codec::write_document(self, writer, options)
    }
    pub fn validation_issues(&self) -> Vec<ValidationIssue> {
        crate::validate::validate(self, &Default::default())
    }
}

fn cost_items(items: &[CostItem]) -> Box<dyn Iterator<Item = &CostItem> + '_> {
    Box::new(
        items
            .iter()
            .flat_map(|item| std::iter::once(item).chain(cost_items(&item.children))),
    )
}

#[derive(Clone, Copy, Debug)]
pub enum DocumentItem<'a> {
    Procurement(&'a Item),
    Trade(&'a TradeItem),
    Cost(&'a CostItem),
    Quantity(&'a QuantityItem),
}

impl DocumentItem<'_> {
    pub fn identifier(&self) -> &str {
        match self {
            Self::Procurement(i) => &i.oz,
            Self::Trade(i) => &i.id,
            Self::Cost(i) => &i.id,
            Self::Quantity(i) => &i.oz,
        }
    }
}
