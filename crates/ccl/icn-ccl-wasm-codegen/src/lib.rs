//'!' Pass #2: lower `icn-ccl-dsl` structures into an executable opcode stream.

use icn_ccl_dsl::{
    ActionStep, DslModule, IfExpr, Rule, RuleValue,
};
use crate::opcodes::{Opcode, Program};
use serde_json;

pub mod opcodes;
pub mod emit;

pub struct WasmGenerator {
    ops: Vec<Opcode>,
}

impl WasmGenerator {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn generate(mut self, modules: Vec<DslModule>) -> Program { 
        for module in modules {
            self.walk_module(&module);
        }
        Program::new(self.ops) 
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
                self.ops.push(Opcode::BeginSection {
                    kind: "role".to_string(), // Fixed kind for roles
                    title: Some(r.name.clone()),
                });
                if let Some(desc) = &r.description {
                    let json_desc = serde_json::to_string(desc).unwrap_or_else(|_|"\"<serialization error>\"".to_string());
                    self.ops.push(Opcode::SetProperty {
                        key: "description".to_string(),
                        value_json: json_desc,
                    });
                }
                self.walk_rules(&r.attributes); // Process attributes as a list of rules
                self.ops.push(Opcode::EndSection);
            }
            other => self.ops.push(Opcode::Todo(format!("Unhandled DslModule: {:?}", other))),
        }
    }

    fn walk_step(&mut self, step: &ActionStep) {
        match step {
            ActionStep::Metered(m) => {
                let data_json = m.data.as_ref().map(|d| serde_json::to_string(d).unwrap_or_else(|_| "[]".to_string()));
                self.ops.push(Opcode::MintToken {
                    res_type: m.resource_type.clone(),
                    amount: m.amount,
                    recipient: m.recipient.clone(),
                    data: data_json,
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

    /// Walk a vector of `Rule`s and push op-codes
    fn walk_rules(&mut self, rules: &[Rule]) {
        for r in rules {
            match &r.value {
                RuleValue::If(expr) => self.walk_if_expr(expr),

                RuleValue::Range(range) => {
                    self.ops.push(Opcode::BeginSection {
                        kind: format!("range_{}_{}", range.start, range.end),
                        title: Some(r.key.clone()),
                    });
                    self.walk_rules(&range.rules);
                    self.ops.push(Opcode::EndSection);
                }

                RuleValue::Map(kv) => {
                    if is_function_call(kv) {
                        let fn_name = &r.key; 
                        let default_args = RuleValue::List(vec![]);
                        let args_val = kv
                            .iter()
                            .find(|k| k.key == "args")
                            .map(|k| &k.value)
                            .unwrap_or(&default_args);
                        self.walk_function_call(fn_name, args_val);
                    } else {
                        self.walk_rules(kv); 
                    }
                }

                RuleValue::String(_) 
                | RuleValue::Number(_) 
                | RuleValue::Boolean(_) 
                | RuleValue::List(_) => {
                    let json_value = serde_json::to_string(&r.value).unwrap_or_else(|_|"\"<serialization error>\"".to_string());
                    self.ops.push(Opcode::SetProperty {
                        key: r.key.clone(),
                        value_json: json_value,
                    });
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

    // --------------------------------------------------------
    //  Helpers
    // --------------------------------------------------------

    /// Convert a lowered function-call into an opcode
    fn walk_function_call(&mut self, fn_name: &str, args_rule: &RuleValue) {
        let args = match args_rule {
            RuleValue::List(xs) => xs.iter().map(|v| format!("{:?}", v)).collect(),
            other @ RuleValue::Map(_) => vec![format!("{:?}", other)],
            other => vec![format!("{:?}", other)],
        };

        self.ops.push(Opcode::CallHost {
            fn_name: fn_name.to_string(),
            args,
        });
    }
}

// -------------------------------------------------------------------------
//  Utility – recognise the map-structure produced by the lowerer for calls
// -------------------------------------------------------------------------

fn is_function_call(kv: &[Rule]) -> bool {
    kv.first()
        .map(|first| first.key == "function_name")
        .unwrap_or(false)
}

pub fn hash32(s: &str) -> u32 {
    // Simple FNV-1a hash for demo purposes
    let mut hash = 0x811c9dc5_u32;
    for byte in s.as_bytes() {
        hash ^= *byte as u32;
        hash = hash.wrapping_mul(0x01000193_u32);
    }
    hash
}

pub fn compile_to_wasm(modules: Vec<DslModule>) -> Vec<u8> {
    let prog = WasmGenerator::new().generate(modules);
    emit::program_to_wasm(&prog)
} 