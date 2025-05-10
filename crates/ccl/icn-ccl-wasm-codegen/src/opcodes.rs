//'!' Extremely-rough first cut at a host-call opcode list.
//'!' Everything will get revisited once we know the real WASM ABI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Opcode {
    /// Placeholder emitted for un-lowered DSL modules.
    Todo { note: String },

    /// Mint a token of `resource_type` to `recipient`, with optional embedded JSON (`data`).
    MintToken {
        resource_type: String,
        recipient: String,
    },

    /// Anchor `data_reference` (CID or pointer) under `path`.
    AnchorData {
        path: String,
        data_reference: String,
    },

    /// Stub for branching – the second `u32` is the jump-target index in `Program.ops`.
    If { condition_raw: String, jump_on_false: u32 },

    // (More opcodes will land here very soon…)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Program {
    pub ops: Vec<Opcode>,
}

impl Program {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }
} 