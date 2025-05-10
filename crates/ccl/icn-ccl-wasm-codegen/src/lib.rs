//'!' Pass #2: lower `icn-ccl-dsl` structures into an executable opcode stream.

use icn_ccl_dsl::{ActionStep, DslModule, Rule, RuleValue, IfExpr};
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
                    version: Some(p.version.clone()),
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
            ActionStep::Metered(m) => {
                self.ops.push(Opcode::MintToken {
                    res_type: m.resource_type.clone(),
                    amount: m.amount,
                    recipient: m.recipient.clone(),
                });
            }
            ActionStep::Anchor(a) => {
                self.ops.push(Opcode::AnchorData {
                    path: a.path.clone(),
                    data_ref: a.data_reference.clone(),
                });
            }
        }
    }

    fn walk_rules(&mut self, rules: &[Rule]) {
        for r in rules {
            match &r.value {
                RuleValue::If(ifx) => self.walk_if_expr(ifx),
                RuleValue::Range(range) => {
                    self.ops.push(Opcode::BeginSection {
                        kind: format!("range_{}_{}", range.start, range.end),
                        title: None,
                    });
                    self.walk_rules(&range.rules);
                    self.ops.push(Opcode::EndSection);
                }
                _ => {
                    self.ops.push(Opcode::Todo(format!("Unhandled DslRule in walk_rules: {} = {:?}", r.key, r.value)));
                }
            }
        }
    }

    /// emit If / Else / EndIf
    fn walk_if_expr(&mut self, ifx: &IfExpr) {
        self.ops.push(Opcode::If {
            condition: ifx.condition_raw.clone(),
        });
        self.walk_rules(&ifx.then_rules);

        if let Some(else_rules) = &ifx.else_rules {
            self.ops.push(Opcode::Else);
            self.walk_rules(else_rules);
        }
        self.ops.push(Opcode::EndIf);
    }
} 