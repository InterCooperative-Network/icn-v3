//'!' Pass #2: lower `icn-ccl-dsl` structures into an executable opcode stream.

mod opcodes;
pub use opcodes::{Opcode, Program};

use icn_ccl_dsl::*;
use Program as P;

pub struct WasmGenerator;

impl WasmGenerator {
    pub fn generate(modules: &[DslModule]) -> P {
        let mut prog = P::new();

        for m in modules {
            match m {
                // ───────────────────────────────────────────────────
                DslModule::ActionHandler(handler) => {
                    for step in &handler.steps {
                        match step {
                            ActionStep::Metered(ma) => prog.ops.push(Opcode::MintToken {
                                resource_type: ma.resource_type.clone(),
                                recipient: ma
                                    .recipient
                                    .clone()
                                    .unwrap_or_else(|| "<unknown>".into()),
                            }),
                            ActionStep::Anchor(a) => prog.ops.push(Opcode::AnchorData {
                                path: a.path.clone().unwrap_or_default(),
                                data_reference: a.data_reference.clone(),
                            }),
                        }
                    }
                }

                DslModule::Proposal(p) => prog.ops.push(Opcode::Todo {
                    note: format!("TODO: proposal \"{}\"", p.title),
                }),

                DslModule::Role(r) => prog.ops.push(Opcode::Todo {
                    note: format!("TODO: role \"{}\"", r.name),
                }),

                DslModule::Section(s) => prog.ops.push(Opcode::Todo {
                    note: format!("TODO: section kind={}", s.kind),
                }),
                // Catch-all for DslModule variants not yet handled above
                // (Vote, Anchor, MeteredAction if they were to appear top-level)
                _ => prog.ops.push(Opcode::Todo {
                    note: format!("TODO: Unhandled DslModule variant: {:?}", m)
                })
            }
        }

        prog
    }
} 