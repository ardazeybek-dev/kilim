use crate::op::{Op, OpId, VersionVector};
use serde::Serialize;
use std::collections::HashSet;

/// One character slot in the replicated sequence. Deleted characters stay in
/// the list as tombstones so that concurrent inserts can still find their origin.
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub id: OpId,
    pub lamport: u64,
    pub ch: char,
    pub deleted: bool,
}

impl Item {
    /// Total order used to place concurrent inserts that share an origin.
    fn key(&self) -> (u64, u32) {
        (self.lamport, self.id.site)
    }
}

/// A change to the visible text caused by applying an operation. Editors use
/// it to update the screen and shift the local cursor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Patch {
    Insert { index: usize, ch: char },
    Delete { index: usize },
}

/// A replica of the shared text.
#[derive(Clone, Debug)]
pub struct Doc {
    site: u32,
    seq: u32,
    lamport: u64,
    items: Vec<Item>,
    /// Ids of every applied operation (inserts and deletes).
    seen: HashSet<OpId>,
    version: VersionVector,
    /// Every applied operation, in application order. Enough to rebuild the
    /// document or to bring another replica up to date.
    log: Vec<Op>,
    /// Operations that arrived before the operation they depend on.
    pending: Vec<Op>,
}

impl Doc {
    /// Creates an empty replica. `site` must be unique among collaborating replicas.
    pub fn new(site: u32) -> Self {
        Doc {
            site,
            seq: 0,
            lamport: 0,
            items: Vec::new(),
            seen: HashSet::new(),
            version: VersionVector::new(),
            log: Vec::new(),
            pending: Vec::new(),
        }
    }

    pub fn site(&self) -> u32 {
        self.site
    }

    /// The current visible text.
    pub fn text(&self) -> String {
        self.visible().map(|it| it.ch).collect()
    }

    /// Number of visible characters.
    pub fn len(&self) -> usize {
        self.visible().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Internal sequence including tombstones (for inspection and visualisation).
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    pub fn version(&self) -> &VersionVector {
        &self.version
    }

    /// Number of received operations still waiting for a dependency.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Inserts `text` at visible character position `pos` and returns the
    /// operations to broadcast to other replicas.
    pub fn insert(&mut self, pos: usize, text: &str) -> Vec<Op> {
        let pos = pos.min(self.len());
        let mut origin = if pos == 0 {
            None
        } else {
            self.visible().nth(pos - 1).map(|it| it.id)
        };
        let mut ops = Vec::new();
        for ch in text.chars() {
            let id = self.next_id();
            self.lamport += 1;
            let op = Op::Insert {
                id,
                lamport: self.lamport,
                origin,
                ch,
            };
            self.apply(op.clone());
            ops.push(op);
            origin = Some(id);
        }
        ops
    }

    /// Deletes `len` visible characters starting at `pos` and returns the
    /// operations to broadcast.
    pub fn delete(&mut self, pos: usize, len: usize) -> Vec<Op> {
        let targets: Vec<OpId> = self.visible().skip(pos).take(len).map(|it| it.id).collect();
        targets
            .into_iter()
            .map(|target| {
                let id = self.next_id();
                self.lamport += 1;
                let op = Op::Delete {
                    id,
                    lamport: self.lamport,
                    target,
                };
                self.apply(op.clone());
                op
            })
            .collect()
    }

    /// Applies a remote operation. Duplicates are ignored and operations whose
    /// dependency has not arrived yet are buffered, so delivery order does not
    /// matter. Returns the visible changes this caused (possibly including
    /// buffered operations that became ready).
    pub fn apply(&mut self, op: Op) -> Vec<Patch> {
        let mut patches = Vec::new();
        if self.seen.contains(&op.id()) {
            return patches;
        }
        if !self.is_ready(&op) {
            if !self.pending.iter().any(|p| p.id() == op.id()) {
                self.pending.push(op);
            }
            return patches;
        }
        patches.extend(self.integrate(op));
        self.drain_pending(&mut patches);
        patches
    }

    /// Applies many operations; convenience for syncing.
    pub fn apply_all<I: IntoIterator<Item = Op>>(&mut self, ops: I) -> Vec<Patch> {
        ops.into_iter().flat_map(|op| self.apply(op)).collect()
    }

    /// Operations a replica at version `since` has not seen yet.
    /// May include a few it already has; applying those is a no-op.
    pub fn ops_since(&self, since: &VersionVector) -> Vec<Op> {
        self.log
            .iter()
            .filter(|op| {
                let id = op.id();
                id.seq > since.get(&id.site).copied().unwrap_or(0)
            })
            .cloned()
            .collect()
    }

    /// A stable reference to a cursor position: the id of the character just
    /// before `pos` (`None` = start). Unlike a plain index it stays correct
    /// while remote edits shift the text around it.
    pub fn anchor(&self, pos: usize) -> Option<OpId> {
        if pos == 0 {
            None
        } else {
            self.visible().nth(pos - 1).map(|it| it.id)
        }
    }

    /// Current visible position of a cursor created with [`Doc::anchor`].
    /// If the anchor character was deleted the cursor collapses onto the gap.
    pub fn resolve(&self, anchor: Option<OpId>) -> usize {
        let Some(id) = anchor else { return 0 };
        match self.position(id) {
            Some(i) => self.items[..=i].iter().filter(|it| !it.deleted).count(),
            None => 0,
        }
    }

    fn next_id(&mut self) -> OpId {
        self.seq += 1;
        OpId {
            site: self.site,
            seq: self.seq,
        }
    }

    fn visible(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|it| !it.deleted)
    }

    fn position(&self, id: OpId) -> Option<usize> {
        self.items.iter().position(|it| it.id == id)
    }

    fn is_ready(&self, op: &Op) -> bool {
        match op {
            Op::Insert { origin: None, .. } => true,
            Op::Insert {
                origin: Some(origin),
                ..
            } => self.seen.contains(origin),
            Op::Delete { target, .. } => self.seen.contains(target),
        }
    }

    /// Core of RGA. Assumes `op` is new and its dependency is present.
    fn integrate(&mut self, op: Op) -> Option<Patch> {
        self.lamport = self.lamport.max(op.lamport());
        let patch = match op.clone() {
            Op::Insert {
                id,
                lamport,
                origin,
                ch,
            } => {
                let item = Item {
                    id,
                    lamport,
                    ch,
                    deleted: false,
                };
                let mut i = match origin {
                    None => 0,
                    Some(o) => self.position(o).expect("origin is present") + 1,
                };
                // Concurrent inserts after the same origin: the newer one (higher
                // lamport, then higher site) goes first. Everything inserted after
                // a skipped item has an even higher lamport, so its whole subtree
                // is skipped too — every replica computes the same position.
                while i < self.items.len() && self.items[i].key() > item.key() {
                    i += 1;
                }
                let index = self.items[..i].iter().filter(|it| !it.deleted).count();
                self.items.insert(i, item);
                Some(Patch::Insert { index, ch })
            }
            Op::Delete { target, .. } => {
                let i = self.position(target).expect("target is present");
                if self.items[i].deleted {
                    None // concurrent delete of the same char
                } else {
                    self.items[i].deleted = true;
                    let index = self.items[..i].iter().filter(|it| !it.deleted).count();
                    Some(Patch::Delete { index })
                }
            }
        };
        self.mark_seen(op.id());
        self.log.push(op);
        patch
    }

    fn mark_seen(&mut self, id: OpId) {
        self.seen.insert(id);
        let v = self.version.entry(id.site).or_insert(0);
        while self.seen.contains(&OpId {
            site: id.site,
            seq: *v + 1,
        }) {
            *v += 1;
        }
    }

    fn drain_pending(&mut self, patches: &mut Vec<Patch>) {
        loop {
            let Some(i) = self.pending.iter().position(|op| self.is_ready(op)) else {
                return;
            };
            let op = self.pending.swap_remove(i);
            if !self.seen.contains(&op.id()) {
                patches.extend(self.integrate(op));
            }
        }
    }
}
