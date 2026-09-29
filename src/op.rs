use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Globally unique id of an operation: the replica that created it and a
/// per-replica sequence number starting at 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct OpId {
    pub site: u32,
    pub seq: u32,
}

/// A single edit. Operations are idempotent and commutative once their
/// causal dependencies are present, which is what makes the document converge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t")]
pub enum Op {
    /// Insert `ch` directly after `origin` (`None` = start of document).
    #[serde(rename = "i")]
    Insert {
        id: OpId,
        /// Lamport clock; together with `id.site` it orders concurrent inserts.
        lamport: u64,
        origin: Option<OpId>,
        ch: char,
    },
    /// Mark the character created by `target` as deleted (tombstone).
    #[serde(rename = "d")]
    Delete {
        id: OpId,
        lamport: u64,
        target: OpId,
    },
}

impl Op {
    pub fn id(&self) -> OpId {
        match self {
            Op::Insert { id, .. } | Op::Delete { id, .. } => *id,
        }
    }

    pub fn lamport(&self) -> u64 {
        match self {
            Op::Insert { lamport, .. } | Op::Delete { lamport, .. } => *lamport,
        }
    }
}

/// For every replica: how many of its operations (counted from seq 1 without
/// gaps) this document has applied. Used to ask a peer "what am I missing?".
pub type VersionVector = BTreeMap<u32, u32>;
