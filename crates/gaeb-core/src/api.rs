use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Document, DocumentItem, DocumentKind, ExchangePhase, ItemType, ValidationSeverity};

pub struct DocumentApi<'a> {
    document: &'a Document,
}

impl<'a> DocumentApi<'a> {
    pub fn new(document: &'a Document) -> Self {
        Self { document }
    }

    pub fn document(&self) -> &'a Document {
        self.document
    }

    pub const fn document_kind(&self) -> DocumentKind {
        self.document.kind()
    }

    pub const fn is_trade(&self) -> bool {
        matches!(self.document, Document::Trade(_))
    }

    pub const fn is_procurement(&self) -> bool {
        matches!(self.document, Document::Procurement(_))
    }

    pub const fn is_cost(&self) -> bool {
        matches!(self.document, Document::Cost(_))
    }

    pub const fn is_quantity(&self) -> bool {
        matches!(self.document, Document::Quantity(_))
    }

    pub fn iter_items(&self) -> impl Iterator<Item = DocumentItem<'a>> {
        self.document.items()
    }

    pub fn find_item(&self, oz: &str) -> Option<DocumentItem<'a>> {
        self.document
            .items()
            .find(|item| matches!(item, DocumentItem::Procurement(item) if item.oz == oz))
    }

    pub fn find_order_item(&self, id: &str) -> Option<DocumentItem<'a>> {
        self.document
            .items()
            .find(|item| matches!(item, DocumentItem::Trade(item) if item.id == id))
    }

    pub fn find_cost_element(&self, id: &str) -> Option<DocumentItem<'a>> {
        self.document
            .items()
            .find(|item| matches!(item, DocumentItem::Cost(item) if item.id == id))
    }

    pub fn find_quantity_item(&self, oz: &str) -> Option<DocumentItem<'a>> {
        self.document
            .items()
            .find(|item| matches!(item, DocumentItem::Quantity(item) if item.oz == oz))
    }

    pub fn filter_items<F>(&self, mut predicate: F) -> Vec<DocumentItem<'a>>
    where
        F: FnMut(DocumentItem<'a>) -> bool,
    {
        self.document
            .items()
            .filter(|item| predicate(*item))
            .collect()
    }

    pub fn filter_item_type(&self, item_type: ItemType) -> Vec<DocumentItem<'a>> {
        self.filter_items(
            |item| matches!(item, DocumentItem::Procurement(item) if item.item_type == item_type),
        )
    }

    pub fn summary(&self) -> DocumentSummary {
        let issues = self.document.validation_issues();
        DocumentSummary {
            source_version: self.document.source_version(),
            exchange_phase: self.document.phase(),
            document_kind: self.document.kind(),
            total_items: self.document.item_count(),
            grand_total: self.document.grand_total(),
            validation_errors: issues
                .iter()
                .filter(|issue| issue.severity == ValidationSeverity::Error)
                .count(),
            validation_warnings: issues
                .iter()
                .filter(|issue| issue.severity == ValidationSeverity::Warning)
                .count(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DocumentSummary {
    pub source_version: crate::SourceVersion,
    pub exchange_phase: ExchangePhase,
    pub document_kind: DocumentKind,
    pub total_items: usize,
    #[serde(with = "rust_decimal::serde::str")]
    pub grand_total: rust_decimal::Decimal,
    pub validation_errors: usize,
    pub validation_warnings: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ItemIndex {
    pub by_identifier: BTreeMap<String, usize>,
    pub duplicates: BTreeMap<String, usize>,
}

impl ItemIndex {
    pub fn build(document: &Document) -> Self {
        let mut index = Self::default();
        for (position, item) in document.items().enumerate() {
            let identifier = item.identifier().to_owned();
            if index
                .by_identifier
                .insert(identifier.clone(), position)
                .is_some()
            {
                *index.duplicates.entry(identifier).or_insert(1) += 1;
            }
        }
        index
    }
}
