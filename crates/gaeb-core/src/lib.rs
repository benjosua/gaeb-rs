#![forbid(unsafe_code)]
//! GAEB DA XML and GAEB 90 codec.
//!
//! Core has no filesystem, network, browser, or adapter dependency. Parse bytes
//! into a normalized domain model, validate it, then write an explicit target
//! version and exchange phase.

mod codec;
mod error;
mod model;
mod options;
mod validate;

pub mod api;
pub mod bid_analysis;
pub mod builder;
pub mod cache;
pub mod converter;
pub mod diff;
pub mod export;
pub mod quality;
pub mod reb;
pub mod transition;
pub mod tree;
pub mod units;
pub mod xrechnung;

pub use builder::{BoqBuilder, CategoryBuilder, ItemBuilder, LotBuilder};
pub use diff::{
    DiffDocInfo, DiffMode, DiffResult, DiffSummary, DuplicateOz, FieldChange, ItemAdded,
    ItemDiffSummary, ItemModified, ItemMoved, ItemRemoved, MetadataChange, SectionChange,
    SectionRenamed, Significance, StructureDiffSummary, compare as compare_documents,
    compare_with_mode,
};
pub use error::GaebError;
pub use export::{diff_to_excel_csv_string, diff_to_html_string};
pub use model::{
    Attachment, BidderPrice, Boq, BoqCategory, ClassificationResult, CostApproach, CostBreakdown,
    CostCategory, CostCategoryElement, CostConsortiumMember, CostDimensionElement, CostDocument,
    CostInfo, CostItem, CostProperty, CostReference, CostReferenceGroup, CostType, CtlgAssign,
    Document, DocumentItem, DocumentKind, DocumentMeta, ExchangePhase, ExtractionResult,
    InvoiceAddress, InvoiceBank, InvoiceCheckNote, InvoiceDocument, InvoiceHeader, InvoiceIssued,
    InvoiceParty, InvoicePayment, InvoicePaymentTerm, InvoiceShare, InvoiceSite, InvoiceText, Item,
    ItemType, Lot, MarkupSubQty, ProcurementDocument, ProjectInfo, QtySplit, QuantityAttachment,
    QuantityCatalog, QuantityCategory, QuantityDeterminationItem, QuantityDocument, QuantityItem,
    RichText, SourceVersion, StandardService, StructuredDocument, SubDescription, TextComplement,
    TradeAddress, TradeDocument, TradeItem, TradeParty, XmlElement,
};
pub use options::{ParseOptions, ValidationOptions, WriteOptions, WriteReport};
pub use validate::{ValidationIssue, ValidationSeverity, validate};
pub use xrechnung::{XRechnungOptions, to_xrechnung, to_xrechnung_bytes, to_xrechnung_string};

/// Parse a DA XML 2.x/3.x or GAEB 90 document.
pub fn parse_bytes(input: &[u8], options: &ParseOptions) -> Result<Document, GaebError> {
    codec::parse_bytes(input, options)
}
