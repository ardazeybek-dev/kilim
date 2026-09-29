//! JavaScript bindings. Positions and lengths are UTF-16 code units, exactly
//! like JS string indices; operations and version vectors travel as JSON.

use crate::{Doc, Op, OpId, VersionVector};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = Doc)]
pub struct WasmDoc {
    doc: Doc,
}

#[wasm_bindgen(js_class = Doc)]
impl WasmDoc {
    /// `site` must be unique per replica (e.g. a random 32-bit integer).
    #[wasm_bindgen(constructor)]
    pub fn new(site: u32) -> WasmDoc {
        WasmDoc {
            doc: Doc::new(site),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn site(&self) -> u32 {
        self.doc.site()
    }

    pub fn text(&self) -> String {
        self.doc.text()
    }

    /// Inserts text and returns the operations to broadcast (JSON array).
    pub fn insert(&mut self, pos: usize, text: &str) -> String {
        let pos = self.to_char(pos);
        to_json(&self.doc.insert(pos, text))
    }

    /// Deletes `len` UTF-16 units at `pos`; returns operations (JSON array).
    pub fn delete(&mut self, pos: usize, len: usize) -> String {
        let start = self.to_char(pos);
        let end = self.to_char(pos + len);
        to_json(&self.doc.delete(start, end - start))
    }

    /// Replaces the whole text with `next`, emitting the minimal
    /// delete + insert around the changed middle. Handy for `<textarea>` input events.
    #[wasm_bindgen(js_name = applyText)]
    pub fn apply_text(&mut self, next: &str) -> String {
        let old: Vec<char> = self.doc.text().chars().collect();
        let new: Vec<char> = next.chars().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let max_suffix = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suffix)
            .take_while(|(a, b)| a == b)
            .count();
        let mut ops = self.doc.delete(prefix, old.len() - prefix - suffix);
        let inserted: String = new[prefix..new.len() - suffix].iter().collect();
        ops.extend(self.doc.insert(prefix, &inserted));
        to_json(&ops)
    }

    /// Applies remote operations (JSON array). Order and duplicates do not matter.
    /// Returns how many of them changed the visible text.
    #[wasm_bindgen(js_name = applyRemote)]
    pub fn apply_remote(&mut self, ops_json: &str) -> Result<usize, JsError> {
        let ops: Vec<Op> = serde_json::from_str(ops_json)?;
        Ok(self.doc.apply_all(ops).len())
    }

    /// Version vector as JSON, e.g. `{"17":42}`.
    pub fn version(&self) -> String {
        to_json(self.doc.version())
    }

    /// Operations missing from a peer at version `version_json`.
    #[wasm_bindgen(js_name = opsSince)]
    pub fn ops_since(&self, version_json: &str) -> Result<String, JsError> {
        let since: VersionVector = serde_json::from_str(version_json)?;
        Ok(to_json(&self.doc.ops_since(&since)))
    }

    /// Stable cursor reference (JSON) for a UTF-16 position.
    pub fn anchor(&self, pos: usize) -> String {
        to_json(&self.doc.anchor(self.to_char(pos)))
    }

    /// Current UTF-16 position of an anchor produced by [`WasmDoc::anchor`].
    pub fn resolve(&self, anchor_json: &str) -> Result<usize, JsError> {
        let anchor: Option<OpId> = serde_json::from_str(anchor_json)?;
        let chars = self.doc.resolve(anchor);
        Ok(self
            .doc
            .text()
            .chars()
            .take(chars)
            .map(char::len_utf16)
            .sum())
    }

    /// Internal sequence including tombstones (JSON) — for visualisation.
    pub fn items(&self) -> String {
        to_json(self.doc.items())
    }

    #[wasm_bindgen(js_name = pendingCount)]
    pub fn pending_count(&self) -> usize {
        self.doc.pending_len()
    }

    fn to_char(&self, utf16: usize) -> usize {
        let mut units = 0;
        let mut chars = 0;
        for ch in self.doc.text().chars() {
            if units >= utf16 {
                break;
            }
            units += ch.len_utf16();
            chars += 1;
        }
        chars
    }
}

fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("serializable")
}
