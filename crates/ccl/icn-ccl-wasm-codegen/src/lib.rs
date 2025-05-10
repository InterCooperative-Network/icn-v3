//'!' Pass #2: lower `icn-ccl-dsl` structures into an executable opcode stream.

use icn_ccl_dsl::{ActionStep, DslModule, DslRule, DslValue};
// Removed DslValue from here as it's used qualified like DslValue::If
use crate::opcodes::{Opcode, Program};

pub mod opcodes;

pub struct WasmGenerator {
    ops: Vec<Opcode>,
}

impl WasmGenerator {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn generate(mut self, modules: Vec<DslModule>) -> Program { // Changed to take Vec, not slice, and consume self
        for module in modules {
            self.walk_module(&module);
        }
        Program::new(self.ops) // Program::new now takes ops
    }

    fn walk_module(&mut self, m: &DslModule) {
        match m {
            DslModule::Proposal(p) => {
                self.ops.push(Opcode::CreateProposal {
                    title: p.title.clone(),
                    version: p.version.clone(), 
                });
                self.walk_rules(&p.rules);
            }
            DslModule::ActionHandler(h) => {
                self.ops.push(Opcode::OnEvent { event: h.event.clone() });
                for step in &h.steps {
                    self.walk_step(step);
                }
            }
            DslModule::Section(s) => {
                self.ops.push(Opcode::BeginSection {
                    kind: s.kind.clone(),
                    title: s.title.clone(),
                });
                self.walk_rules(&s.rules);
                self.ops.push(Opcode::EndSection);
            }
            DslModule::Role(r) => {
                self.ops.push(Opcode::Todo(format!("Role definition: {}", r.name)));
                // If Role has rules, uncomment and ensure DslRule.rules exists:
                // self.walk_rules(&r.rules);
            }
            other => self.ops.push(Opcode::Todo(format!("Unhandled DslModule: {:?}", other))),
        }
    }

    fn walk_step(&mut self, step: &ActionStep) {
        match step {
            ActionStep::MintToken(mt) => {
                self.ops.push(Opcode::Todo(format!("ActionStep::MintToken to be implemented for: {:?}", mt.token_type)));
            }
            ActionStep::AnchorData(ad) => {
                self.ops.push(Opcode::Todo(format!("ActionStep::AnchorData to be implemented for path: {:?}", ad.path)));
            }
            ActionStep::PerformMeteredAction(pma) => {
                self.ops.push(Opcode::Todo(format!("ActionStep::PerformMeteredAction to be implemented for: {:?}", pma.action)));
            }
            // Removed the catch-all _ => as the previous match was exhaustive. 
            // If new ActionStep variants are added, they'll cause a compile error here, which is good.
        }
    }

    fn walk_rules(&mut self, rules: &[DslRule]) {
        for rule in rules {
            match &rule.value {
                DslValue::If(if_expr) => {
                    self.ops.push(Opcode::Todo(format!("IfExpr to be implemented for condition: {:?}", if_expr.condition_raw)));
                }
                DslValue::Range(range_rule) => {
                    self.ops.push(Opcode::Todo(format!("RangeRule to be implemented: {}-{}", range_rule.start, range_rule.end)));
                }
                _ => {
                    self.ops.push(Opcode::Todo(format!("Unhandled DslRule in walk_rules: {} = {:?}", rule.key, rule.value)));
                }
            }
        }
    }
} 