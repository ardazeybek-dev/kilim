//! # kilim
//!
//! A small sequence CRDT for collaborative text editing, based on RGA
//! (Replicated Growable Array).
//!
//! Every replica can edit its own copy offline. Operations are exchanged in
//! any order, any number of times, and every replica that has seen the same
//! set of operations ends up with exactly the same text — no central server,
//! no locking, no conflicts to resolve by hand.
//!
//! ```
//! use kilim::Doc;
//!
//! let mut alice = Doc::new(1);
//! let mut bob = Doc::new(2);
//!
//! let ops = alice.insert(0, "hello");
//! bob.apply_all(ops);
//!
//! // Both edit the same spot concurrently.
//! let a = alice.insert(5, " world");
//! let b = bob.insert(5, "!");
//!
//! alice.apply_all(b);
//! bob.apply_all(a);
//! assert_eq!(alice.text(), bob.text());
//! ```
//!
//! Positions in the Rust API are measured in Unicode scalar values (`char`s).
//! The WebAssembly bindings convert from/to UTF-16 so JavaScript string
//! indices can be used directly.

mod doc;
mod op;

#[cfg(target_arch = "wasm32")]
mod wasm;

pub use doc::{Doc, Item, Patch};
pub use op::{Op, OpId, VersionVector};
