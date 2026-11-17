use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Boq, Lot};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Root,
    Lot,
    Category,
    Item,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BoqNode {
    pub kind: NodeKind,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub depth: usize,
    pub index: usize,
    pub rno: String,
    pub oz: String,
    pub label: String,
}

impl BoqNode {
    pub fn is_root(&self) -> bool {
        self.parent.is_none()
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

#[derive(Clone, Debug, Default)]
pub struct BoqTree {
    nodes: Vec<BoqNode>,
    item_by_oz: BTreeMap<String, Vec<usize>>,
    category_by_rno: BTreeMap<String, Vec<usize>>,
}

impl BoqTree {
    pub fn new(boq: &Boq) -> Self {
        let mut tree = Self::default();
        tree.nodes.push(BoqNode {
            kind: NodeKind::Root,
            parent: None,
            children: Vec::new(),
            depth: 0,
            index: 0,
            rno: String::new(),
            oz: String::new(),
            label: "BoQ".to_owned(),
        });
        if boq.lots.is_empty() {
            for (index, category) in boq.categories.iter().enumerate() {
                tree.add_category(category, 0, 1, index, Vec::new());
            }
        } else {
            for (index, lot) in boq.lots.iter().enumerate() {
                tree.add_lot(lot, 0, index);
            }
        }
        tree
    }

    pub fn root(&self) -> &BoqNode {
        &self.nodes[0]
    }

    pub fn node(&self, index: usize) -> Option<&BoqNode> {
        self.nodes.get(index)
    }

    pub fn walk(&self) -> impl Iterator<Item = (usize, &BoqNode)> {
        self.nodes.iter().enumerate()
    }

    pub fn item_count(&self) -> usize {
        self.nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Item)
            .count()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn find_item(&self, oz: &str) -> Option<(usize, &BoqNode)> {
        self.item_by_oz
            .get(oz)
            .and_then(|indices| indices.first())
            .and_then(|index| self.nodes.get(*index).map(|node| (*index, node)))
    }

    pub fn find_items(&self, oz: &str) -> Vec<(usize, &BoqNode)> {
        self.item_by_oz
            .get(oz)
            .into_iter()
            .flatten()
            .filter_map(|index| self.nodes.get(*index).map(|node| (*index, node)))
            .collect()
    }

    pub fn find_category(&self, rno: &str) -> Option<(usize, &BoqNode)> {
        self.category_by_rno
            .get(rno)
            .and_then(|indices| indices.first())
            .and_then(|index| self.nodes.get(*index).map(|node| (*index, node)))
    }

    fn add_lot(&mut self, lot: &Lot, parent: usize, index: usize) {
        let node_index = self.nodes.len();
        self.nodes.push(BoqNode {
            kind: NodeKind::Lot,
            parent: Some(parent),
            children: Vec::new(),
            depth: 1,
            index,
            rno: lot.rno.clone(),
            oz: String::new(),
            label: lot.label.clone(),
        });
        self.nodes[parent].children.push(node_index);
        for (category_index, category) in lot.categories.iter().enumerate() {
            let path = if lot.rno.is_empty() {
                Vec::new()
            } else {
                vec![lot.rno.clone()]
            };
            self.add_category(category, node_index, 2, category_index, path);
        }
    }

    fn add_category(
        &mut self,
        category: &crate::BoqCategory,
        parent: usize,
        depth: usize,
        index: usize,
        path: Vec<String>,
    ) {
        let node_index = self.nodes.len();
        let mut path = path;
        if !category.rno.is_empty() {
            path.push(category.rno.clone());
        }
        self.nodes.push(BoqNode {
            kind: NodeKind::Category,
            parent: Some(parent),
            children: Vec::new(),
            depth,
            index,
            rno: category.rno.clone(),
            oz: String::new(),
            label: category.label.clone(),
        });
        self.nodes[parent].children.push(node_index);
        self.category_by_rno
            .entry(category.rno.clone())
            .or_default()
            .push(node_index);
        for (child_index, child) in category.categories.iter().enumerate() {
            self.add_category(child, node_index, depth + 1, child_index, path.clone());
        }
        let offset = category.categories.len();
        for (item_index, item) in category.items.iter().enumerate() {
            let index = self.nodes.len();
            let full_oz = if path.is_empty() {
                item.oz.clone()
            } else {
                [path.as_slice(), std::slice::from_ref(&item.oz)]
                    .concat()
                    .join(".")
            };
            self.nodes.push(BoqNode {
                kind: NodeKind::Item,
                parent: Some(node_index),
                children: Vec::new(),
                depth: depth + 1,
                index: offset + item_index,
                rno: item.oz.clone(),
                oz: full_oz.clone(),
                label: item.short_text.clone(),
            });
            self.nodes[node_index].children.push(index);
            self.item_by_oz
                .entry(item.oz.clone())
                .or_default()
                .push(index);
            if full_oz != item.oz {
                self.item_by_oz.entry(full_oz).or_default().push(index);
            }
        }
    }
}
