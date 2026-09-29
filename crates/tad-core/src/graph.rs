//! # Графовая файловая система
//!
//! Хранит RO и направленные рёбра (parent → child через VO).
//! Граф поддерживает циклы (A ссылается на B, B на A) — это нормально,
//! потому что каждый VO — это *ссылка*, а не вложение.
//!
//! Реализация на `sled` (embedded LSM-tree, как RocksDB, но на Rust).
//! Три дерева:
//!   - `ro`:  RoId → RealObject (MessagePack)
//!   - `vo`:  VoId → VirtualObject
//!   - `edges`: RoId → set<VoId>  (реляция parent→child для обхода)

use crate::objects::{RealObject, RoId, VirtualObject, VoId};
use anyhow::Context;
use sled::{Db, Tree};
use std::collections::HashSet;
use std::path::Path;

pub struct GraphStore {
    db: Db,
    ro: Tree,
    vo: Tree,
    edges: Tree,
}

impl GraphStore {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        Self::open_with_db(db)
    }

    pub fn open_with_db(db: sled::Db) -> anyhow::Result<Self> {
        let ro = db.open_tree("ro")?;
        let vo = db.open_tree("vo")?;
        let edges = db.open_tree("edges")?;
        Ok(Self { db, ro, vo, edges })
    }

    pub fn put_ro(&self, ro: &RealObject) -> anyhow::Result<()> {
        let bytes = rmp_serde::to_vec_named(ro)?;
        self.ro.insert(ro.id.as_bytes(), bytes)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_ro(&self, id: RoId) -> anyhow::Result<Option<RealObject>> {
        match self.ro.get(id.as_bytes())? {
            Some(v) => Ok(Some(rmp_serde::from_slice(&v)?)),
            None => Ok(None),
        }
    }

    pub fn delete_ro(&self, id: RoId) -> anyhow::Result<()> {
        self.ro.remove(id.as_bytes())?;
        // Удаляем все VO, указывающие на этот RO.
        let mut to_remove = Vec::new();
        for kv in self.vo.iter() {
            let (k, v) = kv?;
            let vo: VirtualObject = rmp_serde::from_slice(&v)?;
            if vo.target_ro == id || vo.parent_ro == id {
                to_remove.push(k);
            }
        }
        for k in to_remove {
            self.vo.remove(&k)?;
        }
        Ok(())
    }

    pub fn put_vo(&self, vo: &VirtualObject) -> anyhow::Result<()> {
        let bytes = rmp_serde::to_vec_named(vo)?;
        self.vo.insert(vo.id.as_bytes(), bytes)?;
        // Обновляем индекс рёбер parent→child.
        let edge_key = vo.parent_ro.as_bytes().to_vec();
        let mut set: HashSet<Vec<u8>> = self.edges_get(&edge_key)?;
        set.insert(vo.id.as_bytes().to_vec());
        self.edges_put(&edge_key, &set)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn get_vo(&self, id: VoId) -> anyhow::Result<Option<VirtualObject>> {
        match self.vo.get(id.as_bytes())? {
            Some(v) => Ok(Some(rmp_serde::from_slice(&v)?)),
            None => Ok(None),
        }
    }

    pub fn delete_vo(&self, id: VoId) -> anyhow::Result<()> {
        let Some(vo) = self.get_vo(id)? else {
            return Ok(());
        };
        self.vo.remove(id.as_bytes())?;
        let edge_key = vo.parent_ro.as_bytes();
        let mut set = self.edges_get(edge_key)?;
        set.remove(&id.as_bytes().to_vec());
        self.edges_put(edge_key, &set)?;
        self.db.flush()?;
        Ok(())
    }

    /// Move a VO to another parent document (spatial reparenting in the graph).
    pub fn reparent_vo(
        &self,
        id: VoId,
        new_parent: RoId,
        pos: cgmath::Point2<f32>,
    ) -> anyhow::Result<()> {
        let Some(mut vo) = self.get_vo(id)? else {
            return Ok(());
        };
        let old_parent = vo.parent_ro;
        if old_parent == new_parent {
            vo.pos = pos;
            return self.put_vo(&vo);
        }
        self.vo.remove(id.as_bytes())?;
        let old_key = old_parent.as_bytes();
        let mut old_set = self.edges_get(old_key)?;
        old_set.remove(&id.as_bytes().to_vec());
        self.edges_put(old_key, &old_set)?;
        vo.parent_ro = new_parent;
        vo.pos = pos;
        self.put_vo(&vo)
    }

    pub fn all_vo(&self) -> anyhow::Result<Vec<VirtualObject>> {
        let mut out = Vec::new();
        for kv in self.vo.iter() {
            let (_, v) = kv?;
            out.push(rmp_serde::from_slice(&v)?);
        }
        Ok(out)
    }

    /// Все VO, у которых parent_ro = `parent`.
    pub fn children_of(&self, parent: RoId) -> anyhow::Result<Vec<VirtualObject>> {
        let set = self.edges_get(parent.as_bytes())?;
        let mut out = Vec::with_capacity(set.len());
        for k in set {
            if let Some(v) = self.vo.get(&k)? {
                out.push(rmp_serde::from_slice(&v)?);
            }
        }
        Ok(out)
    }

    /// Все VO, которые *ссылаются* на `target` (обратный обход — для порталов).
    pub fn references_to(&self, target: RoId) -> anyhow::Result<Vec<VirtualObject>> {
        let mut out = Vec::new();
        for kv in self.vo.iter() {
            let (_, v) = kv?;
            let vo: VirtualObject = rmp_serde::from_slice(&v)?;
            if vo.target_ro == target {
                out.push(vo);
            }
        }
        Ok(out)
    }

    /// Все RO (для инициализации демо-сцены и отладки).
    pub fn all_ro(&self) -> anyhow::Result<Vec<RealObject>> {
        let mut out = Vec::new();
        for kv in self.ro.iter() {
            let (_, v) = kv?;
            out.push(rmp_serde::from_slice(&v)?);
        }
        Ok(out)
    }

    /// Проверка наличия цикла A→B→A. Нормально для BTRON, но полезно для отладки.
    pub fn has_cycle(&self, a: RoId, b: RoId) -> anyhow::Result<bool> {
        let a_children = self.children_of(a)?;
        let b_children = self.children_of(b)?;
        let a_refs_b = a_children.iter().any(|vo| vo.target_ro == b);
        let b_refs_a = b_children.iter().any(|vo| vo.target_ro == a);
        Ok(a_refs_b && b_refs_a)
    }

    fn edges_get(&self, key: &[u8]) -> anyhow::Result<HashSet<Vec<u8>>> {
        match self.edges.get(key)? {
            Some(v) => Ok(rmp_serde::from_slice(&v).context("decode edges")?),
            None => Ok(HashSet::new()),
        }
    }

    fn edges_put(&self, key: &[u8], set: &HashSet<Vec<u8>>) -> anyhow::Result<()> {
        let bytes = rmp_serde::to_vec_named(set)?;
        self.edges.insert(key, bytes)?;
        Ok(())
    }
}
