use crate::model::{BidderPrice, Document, DocumentItem};
use rust_decimal::Decimal;
use std::collections::BTreeMap;

pub struct BidAnalysis<'a> {
    pub tender: &'a Document,
    pub by_bidder: BTreeMap<String, BTreeMap<String, BidderPrice>>,
    pub duplicates: BTreeMap<String, BTreeMap<String, usize>>,
    pub ranks: BTreeMap<String, usize>,
}

impl<'a> BidAnalysis<'a> {
    pub fn from_x84_bids(tender: &'a Document, bids: BTreeMap<String, &'a Document>) -> Self {
        let mut by_bidder = BTreeMap::new();
        let mut duplicates = BTreeMap::new();

        for (bidder_name, bid_doc) in bids {
            let mut prices = BTreeMap::new();
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();

            for item in bid_doc.items() {
                if let DocumentItem::Procurement(i) = item {
                    let total = i.total_price.or_else(|| i.computed_total());
                    let bp = BidderPrice {
                        bidder_name: bidder_name.clone(),
                        bidder_id: None,
                        unit_price: i.unit_price,
                        total_price: total,
                        affects_total: i.item_type.affects_total(),
                        rank: None,
                    };

                    prices.insert(i.oz.clone(), bp);
                    *seen.entry(i.oz.clone()).or_insert(0) += 1;
                }
            }

            by_bidder.insert(bidder_name.clone(), prices);

            let dups: BTreeMap<_, _> = seen.into_iter().filter(|&(_, n)| n > 1).collect();
            if !dups.is_empty() {
                duplicates.insert(bidder_name, dups);
            }
        }

        let mut analysis = Self {
            tender,
            by_bidder,
            duplicates,
            ranks: BTreeMap::new(),
        };
        analysis.compute_ranks();
        analysis
    }

    pub fn from_x82(doc: &'a Document) -> Self {
        let mut by_bidder: BTreeMap<String, BTreeMap<String, BidderPrice>> = BTreeMap::new();
        let mut seen: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();

        for item in doc.items() {
            if let DocumentItem::Procurement(i) = item {
                for bp in &i.bidder_prices {
                    let prices = by_bidder.entry(bp.bidder_name.clone()).or_default();
                    let mut row = bp.clone();
                    row.affects_total = i.item_type.affects_total();
                    prices.insert(i.oz.clone(), row);

                    let counts = seen.entry(bp.bidder_name.clone()).or_default();
                    *counts.entry(i.oz.clone()).or_insert(0) += 1;
                }
            }
        }

        let duplicates = seen
            .into_iter()
            .filter_map(|(name, counts)| {
                let dups: BTreeMap<_, _> = counts.into_iter().filter(|&(_, n)| n > 1).collect();
                if dups.is_empty() {
                    None
                } else {
                    Some((name, dups))
                }
            })
            .collect();

        let mut analysis = Self {
            tender: doc,
            by_bidder,
            duplicates,
            ranks: BTreeMap::new(),
        };
        analysis.compute_ranks();
        analysis
    }

    fn compute_ranks(&mut self) {
        let ranking = self.ranking();
        for (rank, (name, _)) in ranking.into_iter().enumerate() {
            self.ranks.insert(name, rank + 1);
        }
    }

    pub fn bidders(&self) -> Vec<String> {
        self.by_bidder.keys().cloned().collect()
    }

    pub fn ranking(&self) -> Vec<(String, Decimal)> {
        let mut results: Vec<_> = self
            .by_bidder
            .iter()
            .map(|(name, prices)| {
                let total = prices
                    .values()
                    .filter(|bp| bp.affects_total)
                    .filter_map(|bp| bp.total_price)
                    .sum::<Decimal>();
                let priced_count = prices.values().filter(|bp| bp.unit_price.is_some()).count();
                (name.clone(), total, priced_count == 0)
            })
            .collect();

        results.sort_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1)));
        results.into_iter().map(|(n, t, _)| (n, t)).collect()
    }

    pub fn lowest_bidder(&self) -> Option<String> {
        self.ranking().into_iter().next().map(|(name, _)| name)
    }

    pub fn grand_total(&self, bidder_name: &str) -> Option<Decimal> {
        self.by_bidder.get(bidder_name).map(|prices| {
            prices
                .values()
                .filter(|bp| bp.affects_total)
                .filter_map(|bp| bp.total_price)
                .sum::<Decimal>()
        })
    }

    pub fn duplicates_collapsed(&self, bidder_name: &str) -> BTreeMap<String, usize> {
        self.duplicates
            .get(bidder_name)
            .cloned()
            .unwrap_or_default()
    }

    pub fn priced_item_count(&self, bidder_name: &str) -> usize {
        self.by_bidder
            .get(bidder_name)
            .map(|prices| {
                prices
                    .values()
                    .filter(|price| price.unit_price.is_some())
                    .count()
            })
            .unwrap_or_default()
    }

    pub fn resolve_oz(&self, oz: &str) -> Option<String> {
        self.by_bidder.values().find_map(|prices| {
            prices
                .keys()
                .find(|candidate| candidate.as_str() == oz)
                .cloned()
        })
    }

    pub fn price_spread(&self, oz: &str) -> Option<BTreeMap<String, Decimal>> {
        let mut result = BTreeMap::new();
        for (bidder, prices) in &self.by_bidder {
            if let Some(price) = prices.get(oz).and_then(|price| price.total_price) {
                result.insert(bidder.clone(), price);
            }
        }
        (!result.is_empty()).then_some(result)
    }

    pub fn get_bidder_price(&self, bidder_name: &str, oz: &str) -> Option<&BidderPrice> {
        self.by_bidder.get(bidder_name)?.get(oz)
    }

    pub fn rank(&self, bidder_name: &str) -> Option<usize> {
        self.ranks.get(bidder_name).copied()
    }

    pub fn items_priced_by_all(&self) -> Vec<String> {
        let mut iter = self.by_bidder.values();
        let Some(first) = iter.next() else {
            return Vec::new();
        };
        first
            .keys()
            .filter(|oz| {
                iter.clone().all(|prices| {
                    prices
                        .get(*oz)
                        .is_some_and(|price| price.unit_price.is_some())
                })
            })
            .cloned()
            .collect()
    }
}
