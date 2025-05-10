//'!' Extremely-rough first cut at a host-call opcode list.
//'!' Everything will get revisited once we know the real WASM ABI.

use serde::{Deserialize, Serialize};

// Opcode represents a single operation in a compiled ICN program.
// These are high-level opcodes, not raw WASM instructions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Opcode {
    // high-level
    CreateProposal { title: String, version: Option<String> },
    OnEvent { event: String },

    // actions
    MintToken { res_type: String, amount: u64, recipient: Option<String> },
    AnchorData { path: Option<String>, data_ref: String },
    CallHost { fn_name: String, args: Vec<String> },

    // control flow
    If { condition: String },
    Else,
    EndIf,

    // misc
    RangeCheck { start: f64, end: f64 },
    BeginSection { kind: String, title: Option<String> },
    EndSection,

    /// Simple key/value pair that doesn't warrant its own opcode.
    /// `value_json` is always valid JSON (even for strings – we quote them).
    SetProperty { key: String, value_json: String },
    Todo(String),
}

// Program is a sequence of opcodes, the result of compiling a DslModule list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub ops: Vec<Opcode>,
}

impl Program {
    pub fn new(ops: Vec<Opcode>) -> Self {
        Program { ops }
    }
} 