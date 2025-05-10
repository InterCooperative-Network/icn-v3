use icn_ccl_dsl::{
    ActionHandler, ActionStep, Anchor, DslModule, GenericSection, IfExpr, MeteredAction, Proposal,
    RangeRule, Role as DslAstRole, Rule as DslRule, RuleValue as DslValue,
};
use icn_ccl_parser::{CclParser, Rule};
use pest::iterators::{Pair, Pairs};
use pest::Parser;
use thiserror::Error;
use uuid::Uuid;

// Constant UUID for deterministic test snapshots
#[cfg(test)]
const TEST_UUID_STR: &str = "f0f1f2f3-f4f5-f6f7-f8f9-fafbfcfdfeff"; // Different from DSL test UUID

#[derive(Debug, Error)]
pub enum LowerError {
    #[error("parse error: {0}")]
    Parse(#[from] Box<pest::error::Error<Rule>>),
    #[error("unhandled rule: {0:?}")]
    Unhandled(Pair<'static, Rule>),
}

/// Primary entry‐point used by CLI & tests.
pub fn lower_str(src: &str) -> Result<Vec<DslModule>, LowerError> {
    let mut pairs = CclParser::parse(Rule::ccl, src).map_err(Box::new)?;
    let ccl_root_pair = pairs.next().ok_or_else(|| {
        // This case should ideally not happen if parsing Rule::ccl was successful
        // and the grammar expects at least SOI/EOI or some content.
        // Creating a generic parse error if it does.
        Box::new(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected a CCL root pair but found none.".to_string(),
            },
            pest::Span::new(src, 0, 0).unwrap(), // Dummy span
        ))
    })?;
    Lowerer.lower(ccl_root_pair.into_inner())
}

#[derive(Default)]
struct Lowerer;

impl Lowerer {
    fn lower(&self, pairs: Pairs<'_, Rule>) -> Result<Vec<DslModule>, LowerError> {
        let mut modules = Vec::new();
        for pair in pairs {
            match pair.as_rule() {
                Rule::statement => {
                    for inner in pair.into_inner() {
                        self.dispatch_def(&mut modules, inner)?;
                    }
                }
                _ => {
                    self.dispatch_def(&mut modules, pair)?;
                }
            }
        }
        Ok(modules)
    }

    fn dispatch_def(
        &self,
        modules: &mut Vec<DslModule>,
        pair: Pair<'_, Rule>,
    ) -> Result<(), LowerError> {
        match pair.as_rule() {
            Rule::proposal_def => {
                modules.push(DslModule::Proposal(self.lower_proposal(pair)?));
            }
            Rule::election_def => {
                modules.push(DslModule::Proposal(self.lower_election(pair)?));
            }
            Rule::budget_def => {
                modules.push(DslModule::Proposal(self.lower_proposal(pair)?));
            }
            Rule::bylaws_def => {
                modules.push(DslModule::Proposal(self.lower_bylaws_def(pair)?));
            }
            Rule::roles_def => {
                let pair_span = pair.as_span(); // Get span before move
                                                // roles_def = { "roles" ~ block }
                                                // The block itself is the first inner pair of roles_def
                if let Some(block_pair) = pair.into_inner().next() {
                    if block_pair.as_rule() == Rule::block {
                        self.lower_roles_from_block(block_pair, modules)?;
                    } else {
                        // This case should ideally be prevented by the grammar if roles_def strictly expects a block.
                        // If it can occur, it's an unexpected structure.
                        return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError {
                                message: format!(
                                    "Expected block within roles_def, found {:?}",
                                    block_pair.as_rule()
                                ),
                            },
                            block_pair.as_span(),
                        ))));
                    }
                } else {
                    // roles_def was empty or did not contain a block, also an error.
                    return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "roles_def is empty or missing a block".to_string(),
                        },
                        pair_span, // Use the stored span
                    ))));
                }
            }
            Rule::actions_def => {
                modules.extend(self.lower_actions(pair)?);
            }
            Rule::organization_def
            | Rule::governance_def
            | Rule::membership_def
            | Rule::allocations_def
            | Rule::spending_rules_def
            | Rule::reporting_def
            | Rule::process_def
            | Rule::vacancies_def => {
                modules.push(DslModule::Section(self.lower_generic_section(pair)?));
            }

            Rule::EOI => {} // EOI will be the last item from ccl_root_pair.into_inner()
            _other => {
                // TODO: Review this transmute for safety. It casts a non-'static Pair to 'static.
                // This is only safe if the underlying data for 'pair' outlives its use in LowerError::Unhandled.
                // A better fix might be to store an owned representation.
                return Err(LowerError::Unhandled(unsafe {
                    std::mem::transmute::<Pair<'_, Rule>, Pair<'static, Rule>>(pair)
                }));
            }
        }
        Ok(())
    }

    fn lower_roles_from_block(
        &self,
        block_pair: Pair<'_, Rule>,
        modules: &mut Vec<DslModule>,
    ) -> Result<(), LowerError> {
        // block_pair is Rule::block, containing statements
        for statement_pair in block_pair.into_inner() {
            if statement_pair.as_rule() == Rule::statement {
                // A statement should have one inner actual definition
                if let Some(inner_def_pair) = statement_pair.into_inner().next() {
                    if inner_def_pair.as_rule() == Rule::role_def {
                        let role_dsl = self.lower_single_role_def(inner_def_pair)?;
                        modules.push(DslModule::Role(role_dsl));
                    }
                    // else: other statement types inside roles block (e.g., comments parsed as WHITESPACE, or other valid statements).
                    // For now, we only care about role_def.
                }
            }
            // else: could be WHITESPACE (comments) directly within the block if grammar allows.
        }
        Ok(())
    }

    fn lower_single_role_def(
        &self,
        role_def_pair: Pair<'_, Rule>,
    ) -> Result<DslAstRole, LowerError> {
        // role_def = { "role" ~ string_literal ~ block }
        let pair_span = role_def_pair.as_span(); // Span of the whole role_def for error reporting
        let mut inner_role_pairs = role_def_pair.into_inner();

        // First inner is string_literal (role name)
        let role_name_pair = inner_role_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Role definition missing name".to_string(),
                },
                pair_span, // Error points to the whole role_def
            )))
        })?;
        // Ensure it's a string_literal as expected by grammar `role_def = { "role" ~ string_literal ~ block }`
        // Note: role_name_pair.as_rule() might be Rule::inner_string if string_literal is silent.
        // The grammar for string_literal is `${ """ ~ inner_string ~ """ }`.
        // inner_string is `@ { ... }`. `as_str()` on `string_literal` includes quotes.
        let role_name = role_name_pair.as_str().trim_matches('"').to_string();

        // Second inner is block (role attributes)
        let role_block_pair = inner_role_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Role definition for '{}' missing block", role_name),
                },
                pair_span, // Error points to the whole role_def
            )))
        })?;

        if role_block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block for role '{}', found {:?}"#,
                        role_name,
                        role_block_pair.as_rule()
                    ),
                },
                role_block_pair.as_span(),
            ))));
        }

        let (description, attributes) = self.lower_block_common_fields(role_block_pair)?;

        Ok(DslAstRole {
            name: role_name,
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            attributes,
        })
    }

    fn lower_block_common_fields(
        &self,
        block_pair: Pair<'_, Rule>,
    ) -> Result<(String, Vec<DslRule>), LowerError> {
        let mut description_body = String::new();
        let mut dsl_rules = Vec::<DslRule>::new();
        if block_pair.as_rule() == Rule::block {
            for statement_pair in block_pair.into_inner() {
                if statement_pair.as_rule() == Rule::statement {
                    if let Some(inner_def_pair) = statement_pair.into_inner().next() {
                        match inner_def_pair.as_rule() {
                            Rule::any_statement => {
                                let mut field_parts = inner_def_pair.clone().into_inner(); // Clone for logging if needed
                                let key_pair_opt = field_parts.next();
                                let value_outer_pair_opt = field_parts.next();

                                if let Some(key_pair) = key_pair_opt {
                                    let key_str = key_pair.as_str().trim_matches('"');

                                    if let Some(value_outer_pair) = value_outer_pair_opt {
                                        match value_outer_pair.as_rule() {
                                            Rule::value => {
                                                if let Some(value_inner_pair) =
                                                    value_outer_pair.into_inner().next()
                                                {
                                                    let dsl_val =
                                                        self.lower_value_rule(value_inner_pair)?;
                                                    if key_str == "description" {
                                                        // Assuming description is always a simple string for now
                                                        if let DslValue::String(s) = dsl_val {
                                                            description_body = s;
                                                        } else {
                                                            // Or handle error / log if description is not a string
                                                        }
                                                    } else {
                                                        dsl_rules.push(DslRule {
                                                            key: key_str.to_string(),
                                                            value: dsl_val,
                                                        });
                                                    }
                                                }
                                                // else: Rule::value was empty, ignore for now
                                            }
                                            Rule::block => {
                                                let (_nested_desc, nested_rules) = self
                                                    .lower_block_common_fields(value_outer_pair)?;
                                                dsl_rules.push(DslRule {
                                                    key: key_str.to_string(),
                                                    value: DslValue::Map(nested_rules),
                                                });
                                            }
                                            Rule::general_identifier => {
                                                dsl_rules.push(DslRule {
                                                    key: key_str.to_string(),
                                                    value: DslValue::String(
                                                        value_outer_pair.as_str().to_string(),
                                                    ),
                                                });
                                            }
                                            _ => {
                                                // Should not happen if grammar for any_statement is (value | block | identifier)
                                                // Potentially log an unhandled rule here.
                                            }
                                        }
                                    } else {
                                        // any_statement was just `key;` (no value part), create a boolean true rule or similar?
                                        // For now, if no value_outer_pair, it implies a valueless key. We can represent this
                                        // as a boolean true, or a special Null/Unit type if added to DslValue.
                                        // Let's default to Boolean(true) as a placeholder convention.
                                        dsl_rules.push(DslRule {
                                            key: key_str.to_string(),
                                            value: DslValue::Boolean(true), // Placeholder for valueless keys
                                        });
                                    }
                                }
                            }
                            Rule::range_statement => {
                                let range_rule_data = self.lower_range_statement(inner_def_pair)?;
                                let key = format!(
                                    "range_{}_{}",
                                    range_rule_data.start, range_rule_data.end
                                );
                                dsl_rules.push(DslRule {
                                    key,
                                    value: DslValue::Range(Box::new(range_rule_data)),
                                });
                            }
                            Rule::if_statement => {
                                let if_expr_data = self.lower_if_statement(inner_def_pair)?;
                                // Create a key for the if statement, e.g., based on its condition or a counter
                                // For now, using a generic key placeholder
                                let key = format!("if_condition_{}", dsl_rules.len()); // Simple unique key
                                dsl_rules.push(DslRule {
                                    key,
                                    value: DslValue::If(Box::new(if_expr_data)),
                                });
                            }
                            Rule::function_call_statement => {
                                // function_call_statement = { function_call ~ ";" }
                                // inner_def_pair is Rule::function_call_statement
                                // Its first inner should be Rule::function_call
                                if let Some(fc_pair) = inner_def_pair.into_inner().next() {
                                    if fc_pair.as_rule() == Rule::function_call {
                                        // Extract function name from the function_call pair for the key
                                        // function_call = { identifier ~ "(" ~ function_call_args ~ ")" }
                                        // The first inner of fc_pair is the identifier (function name)
                                        let fn_name =
                                            fc_pair.clone().into_inner().next().map_or_else(
                                                || "unknown_function_call".to_string(),
                                                |p| p.as_str().to_string(),
                                            );

                                        let dsl_val = self.lower_value_rule(fc_pair)?;
                                        dsl_rules.push(DslRule {
                                            key: fn_name,
                                            value: dsl_val,
                                        });
                                    }
                                    // Else: malformed function_call_statement, inner was not function_call
                                }
                                // Else: malformed function_call_statement, no inner pair
                            }
                            _ => { /* Other definitions in statement. Ignored for common fields. */
                            }
                        }
                    }
                }
            }
        }
        Ok((description_body, dsl_rules))
    }

    fn lower_value_rule(&self, value_pair: Pair<'_, Rule>) -> Result<DslValue, LowerError> {
        // value_pair is the actual primitive rule like string_literal, number, boolean, etc.
        // not Rule::value itself.
        match value_pair.as_rule() {
            Rule::string_literal => Ok(DslValue::String(
                value_pair.as_str().trim_matches('"').to_string(),
            )),
            Rule::number => {
                let num_str = value_pair.as_str();
                num_str.parse::<f64>().map(DslValue::Number).map_err(|e| {
                    LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: format!("Invalid number: {}", e),
                        },
                        value_pair.as_span(),
                    )))
                })
            }
            Rule::boolean => {
                // The grammar for boolean is `boolean = { "true" | "false" }`
                // so `as_str()` will give "true" or "false".
                value_pair
                    .as_str()
                    .parse::<bool>()
                    .map(DslValue::Boolean)
                    .map_err(|e| {
                        LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError {
                                message: format!("Invalid boolean: {}", e),
                            },
                            value_pair.as_span(),
                        )))
                    })
            }
            Rule::duration => {
                // For now, treat duration as a string. Could be a specific DslValue variant later.
                Ok(DslValue::String(value_pair.as_str().to_string()))
            }
            Rule::array => {
                // array = { "[" ~ (value ~ ("," ~ value)*)? ~ ","? ~ "]" }
                // Inner pairs of Rule::array will be Rule::value
                let mut elements = Vec::new();
                for element_value_pair in value_pair.into_inner() {
                    // element_value_pair is Rule::value, so we need its inner actual value_inner_pair
                    if let Some(actual_element_val_pair) = element_value_pair.into_inner().next() {
                        elements.push(self.lower_value_rule(actual_element_val_pair)?);
                    }
                }
                Ok(DslValue::List(elements))
            }
            Rule::general_identifier => Ok(DslValue::String(value_pair.as_str().to_string())),
            Rule::object => {
                // object = { "{" ~ (object_pair ~ ("," ~ object_pair)*)? ~ ","? ~ "}" }
                // object_pair = { (string_literal | identifier) ~ ":" ~ value }
                // This is effectively a block of key-value assignments, similar to a Rule::block
                // but specific to the `value` context. We can reuse lower_block_common_fields logic
                // by treating the object_pair items as if they were statements inside a block.
                // lower_block_common_fields expects a Rule::block, which contains Rule::statement(s),
                // and each Rule::statement contains the actual definition (like any_statement or a specific_def).
                // Here, Rule::object contains Rule::object_pair(s).
                // Each object_pair has identifier/string_literal (key) and Rule::value (value).

                let mut dsl_rules = Vec::new();
                for kv_pair in value_pair.into_inner() {
                    // kv_pair is Rule::object_pair
                    if kv_pair.as_rule() == Rule::object_pair {
                        let mut inner_kv = kv_pair.into_inner();
                        let key_obj_pair = inner_kv.next();
                        let value_obj_pair = inner_kv.next();

                        if let (Some(k_pair), Some(v_rule_container_pair)) =
                            (key_obj_pair, value_obj_pair)
                        {
                            // k_pair is string_literal or identifier
                            // v_rule_container_pair is Rule::value, its inner is the actual value type
                            let key_str = k_pair.as_str().trim_matches('"').to_string();
                            if let Some(actual_v_pair) = v_rule_container_pair.into_inner().next() {
                                dsl_rules.push(DslRule {
                                    key: key_str,
                                    value: self.lower_value_rule(actual_v_pair)?,
                                });
                            }
                        }
                    }
                }
                Ok(DslValue::Map(dsl_rules))
            }
            Rule::function_call => {
                // function_call = { identifier ~ "(" ~ function_call_args ~ ")" }
                // function_call_args = { (object_pair ~ ("," ~ object_pair)*)? }
                let original_fn_call_span = value_pair.as_span();
                let mut inner_fc_pairs = value_pair.into_inner();
                let fn_name_pair = inner_fc_pairs.next();
                let fn_args_container_pair = inner_fc_pairs.next(); // This is Rule::function_call_args

                if let Some(name_p) = fn_name_pair {
                    let fn_name = name_p.as_str().to_string();
                    let mut named_args_rules = Vec::new();

                    if let Some(args_container) = fn_args_container_pair {
                        // args_container is Rule::function_call_args
                        // Its inner pairs are Rule::object_pair
                        for arg_object_pair in args_container.into_inner() {
                            if arg_object_pair.as_rule() == Rule::object_pair {
                                // object_pair = { (string_literal | identifier) ~ ":" ~ value }
                                let mut object_pair_inners = arg_object_pair.into_inner();
                                let arg_key_pair = object_pair_inners.next();
                                let arg_value_wrapper_pair = object_pair_inners.next(); // This is Rule::value

                                if let (Some(k_pair), Some(v_wrapper_pair)) =
                                    (arg_key_pair, arg_value_wrapper_pair)
                                {
                                    let arg_key_str = k_pair.as_str().trim_matches('"').to_string();
                                    // v_wrapper_pair is Rule::value, its inner is the actual value type
                                    if let Some(actual_arg_val_pair) =
                                        v_wrapper_pair.into_inner().next()
                                    {
                                        let dsl_arg_val =
                                            self.lower_value_rule(actual_arg_val_pair)?;
                                        named_args_rules.push(DslRule {
                                            key: arg_key_str,
                                            value: dsl_arg_val,
                                        });
                                    }
                                    // Else: Rule::value was empty, or malformed object_pair, ignore for now or error
                                }
                                // Else: Malformed object_pair, ignore for now or error
                            }
                        }
                    }

                    let fn_call_map_rules = vec![
                        DslRule { key: "function_name".to_string(), value: DslValue::String(fn_name) },
                        DslRule { key: "args".to_string(), value: DslValue::Map(named_args_rules) },
                    ];
                    Ok(DslValue::Map(fn_call_map_rules))
                } else {
                    Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "Invalid function call structure: missing function name"#
                                .to_string(),
                        },
                        original_fn_call_span,
                    ))))
                }
            }
            Rule::range_value => {
                // Added for keyed ranges
                // value_pair is Rule::range_value. Its inner pairs (number, number, block)
                // are what lower_range_statement expects.
                let range_rule_data = self.lower_range_statement(value_pair)?;
                Ok(DslValue::Range(Box::new(range_rule_data)))
            }
            _ => Ok(DslValue::String(format!(
                "UNPROCESSED_VALUE_RULE_{:?}_{}",
                value_pair.as_rule(),
                value_pair.as_str()
            ))), // Placeholder
        }
    }

    fn lower_range_statement(&self, pair: Pair<'_, Rule>) -> Result<RangeRule, LowerError> {
        // pair is Rule::range_statement = { "range" ~ number ~ number ~ block }
        let original_span = pair.as_span(); // For top-level error reporting
        let mut inner_pairs = pair.into_inner();

        let start_pair = inner_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Range statement missing start number".to_string(),
                },
                original_span,
            )))
        })?;
        let start_val = start_pair.as_str().parse::<f64>().map_err(|e| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid start number for range: {}", e),
                },
                start_pair.as_span(),
            )))
        })?;

        let end_pair = inner_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Range statement missing end number".to_string(),
                },
                original_span,
            )))
        })?;
        let end_val = end_pair.as_str().parse::<f64>().map_err(|e| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Invalid end number for range: {}", e),
                },
                end_pair.as_span(),
            )))
        })?;

        let block_pair = inner_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Range statement missing block".to_string(),
                },
                original_span,
            )))
        })?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block in range statement, found {:?}"#,
                        block_pair.as_rule()
                    ),
                },
                block_pair.as_span(),
            ))));
        }

        // The description part from lower_block_common_fields is not used for RangeRule's sub-rules.
        let (_description, rules_for_range) = self.lower_block_common_fields(block_pair)?;

        Ok(RangeRule {
            start: start_val,
            end: end_val,
            rules: rules_for_range,
        })
    }

    fn lower_if_statement(&self, pair: Pair<'_, Rule>) -> Result<IfExpr, LowerError> {
        // pair is Rule::if_statement = { "if" ~ comparison_expression ~ block ~ ("else" ~ block)? }
        let original_span = pair.as_span();
        let mut inner_pairs = pair.into_inner();

        let comparison_expr_pair = inner_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "If statement missing condition".to_string(),
                },
                original_span,
            )))
        })?;
        let condition_raw = comparison_expr_pair.as_str().to_string();

        let then_block_pair = inner_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "If statement missing 'then' block".to_string(),
                },
                original_span,
            )))
        })?;

        if then_block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block for 'then' branch, found {:?}"#,
                        then_block_pair.as_rule()
                    ),
                },
                then_block_pair.as_span(),
            ))));
        }
        let (_then_desc, then_rules) = self.lower_block_common_fields(then_block_pair)?;

        let mut else_rules = None;
        // If there's another pair after the 'then' block, it must be the 'else' block.
        // The "else" keyword itself is consumed by Pest and doesn't appear as a separate pair here.
        if let Some(else_block_pair) = inner_pairs.next() {
            if else_block_pair.as_rule() == Rule::block {
                let (_else_desc, rules) = self.lower_block_common_fields(else_block_pair)?;
                else_rules = Some(rules);
            } else {
                // This is an error: if something follows the 'then' block, it must be a block (for 'else')
                return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: format!(
                            "Expected block for 'else' branch, found {:?}"#,
                            else_block_pair.as_rule()
                        ),
                    },
                    else_block_pair.as_span(),
                ))));
            }
        }

        // Ensure there are no more tokens after the optional else block
        if inner_pairs.next().is_some() {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Unexpected tokens after if-statement's else block".to_string(),
                },
                original_span, // Or a more specific span from the unexpected token if available
            ))));
        }

        Ok(IfExpr {
            condition_raw,
            then_rules,
            else_rules,
        })
    }

    fn lower_proposal(&self, pair: Pair<'_, Rule>) -> Result<Proposal, LowerError> {
        let pair_span = pair.as_span();
        let mut proposal_specific_pairs = pair.into_inner();

        let title = proposal_specific_pairs
            .next()
            .ok_or_else(|| {
                LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Proposal missing title".to_string(),
                    },
                    pair_span,
                )))
            })?
            .as_str()
            .trim_matches('"')
            .to_owned();

        // For proposal_def, election_def, budget_def which don't have a version in grammar
        let version = "0.0.0-unknown".to_string(); // Default/placeholder version

        let block_pair = proposal_specific_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Proposal missing block".to_string(),
                },
                pair_span,
            )))
        })?;

        let (description_body, dsl_rules) = self.lower_block_common_fields(block_pair)?;

        Ok(self.build_stub_proposal(title, version, description_body, dsl_rules))
    }

    fn lower_election(&self, pair: Pair<'_, Rule>) -> Result<Proposal, LowerError> {
        let pair_span = pair.as_span();
        let mut election_specific_pairs = pair.into_inner(); // These are specific to election_def

        let title = election_specific_pairs
            .next()
            .ok_or_else(|| {
                LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Election missing title".to_string(),
                    },
                    pair_span,
                )))
            })?
            .as_str()
            .trim_matches('"')
            .to_owned();

        let block_pair = election_specific_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Election missing block".to_string(),
                },
                pair_span,
            )))
        })?;

        let (description_body, dsl_rules) = self.lower_block_common_fields(block_pair)?;

        Ok(self.build_stub_proposal(
            title,
            "0.0.0-unknown".to_string(),
            description_body,
            dsl_rules,
        ))
    }

    fn build_stub_proposal(
        &self,
        title: String,
        version: String,
        body: String,
        rules: Vec<DslRule>,
    ) -> Proposal {
        let id = {
            #[cfg(test)]
            {
                Uuid::parse_str(TEST_UUID_STR).unwrap()
            }
            #[cfg(not(test))]
            {
                Uuid::new_v4()
            }
        };

        Proposal {
            id,
            title,
            version,
            body,
            author: "unknown".into(),
            created_at: 0,
            rules, // Use passed in rules
        }
    }

    fn lower_bylaws_def(&self, pair: Pair<'_, Rule>) -> Result<Proposal, LowerError> {
        // bylaws_def = { "bylaws_def" ~ string_literal ~ "version" ~ string_literal ~ block }
        let pair_span = pair.as_span();
        let mut bylaws_specific_pairs = pair.into_inner();

        let title = bylaws_specific_pairs
            .next()
            .ok_or_else(|| {
                LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Bylaws definition missing title".to_string(),
                    },
                    pair_span,
                )))
            })?
            .as_str()
            .trim_matches('"')
            .to_owned();

        let version = bylaws_specific_pairs
            .next() // Skips "version" keyword, takes the string_literal after it
            .ok_or_else(|| {
                LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                    pest::error::ErrorVariant::CustomError {
                        message: "Bylaws definition missing version string".to_string(),
                    },
                    pair_span,
                )))
            })?
            .as_str()
            .trim_matches('"')
            .to_owned();

        let block_pair = bylaws_specific_pairs.next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "Bylaws definition missing block".to_string(),
                },
                pair_span,
            )))
        })?;

        let (description_body, dsl_rules) = self.lower_block_common_fields(block_pair)?;

        Ok(self.build_stub_proposal(title, version, description_body, dsl_rules))
    }

    fn lower_actions(&self, pair: Pair<'_, Rule>) -> Result<Vec<DslModule>, LowerError> {
        let mut handlers = Vec::new();
        let pair_span = pair.as_span();
        let block_pair = pair.into_inner().next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "actions_def is missing a block".to_string(),
                },
                pair_span, 
            )))
        })?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block within actions_def, found {:?}",
                        block_pair.as_rule()
                    ),
                },
                block_pair.as_span(),
            ))));
        }

        for statement_pair_outer in block_pair.into_inner() {
            // Iterates statements in actions block
            let outer_rule = statement_pair_outer.as_rule();
            let _outer_span = statement_pair_outer.as_span();
            if outer_rule == Rule::statement {
                if let Some(on_pair) = statement_pair_outer.into_inner().next() {
                    let on_pair_rule = on_pair.as_rule();
                    let _on_pair_span_for_log = on_pair.as_span();
                    if on_pair_rule == Rule::action_def {
                        let on_action_def_span = on_pair.as_span();
                        let mut inner_action_def_pairs = on_pair.into_inner();
                        let event_name_pair = inner_action_def_pairs.next().ok_or_else(|| {
                            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: "action_def missing event name (string_literal)"
                                        .to_string(),
                                },
                                on_action_def_span,
                            )))
                        })?;
                        let event = event_name_pair.as_str().trim_matches('"').to_owned();

                        let steps_block_pair = inner_action_def_pairs.next().ok_or_else(|| {
                            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: format!(
                                        "action_def for event '{}' missing block",
                                        event
                                    ),
                                },
                                on_action_def_span,
                            )))
                        })?;

                        if steps_block_pair.as_rule() != Rule::block {
                            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: format!(
                                        "Expected block for action_def event '{}', found {:?}"#,
                                        event,
                                        steps_block_pair.as_rule()
                                    ),
                                },
                                steps_block_pair.as_span(),
                            ))));
                        }

                        let mut steps = Vec::new();
                        for step_statement_pair_outer in steps_block_pair.into_inner() {
                            let step_outer_rule = step_statement_pair_outer.as_rule();
                            let _step_outer_span = step_statement_pair_outer.as_span();
                            if step_outer_rule == Rule::statement {
                                if let Some(step_pair) =
                                    step_statement_pair_outer.into_inner().next()
                                {
                                    match step_pair.as_rule() {
                                        Rule::mint_token => {
                                            steps.push(ActionStep::Metered(
                                                self.lower_mint_token(step_pair)?,
                                            ));
                                        }
                                        Rule::anchor_data => {
                                            steps.push(ActionStep::Anchor(
                                                self.lower_anchor_data(step_pair)?,
                                            ));
                                        }
                                        Rule::perform_metered_action => {
                                            // TODO: Implement lowering for perform_metered_action
                                        }
                                        _ => {
                                            // Other statement types within an action_def block are ignored for now
                                        }
                                    }
                                }
                            }
                        }
                        handlers.push(DslModule::ActionHandler(ActionHandler { event, steps }));
                    }
                }
            }
        }
        Ok(handlers)
    }

    fn lower_mint_token(&self, pair: Pair<'_, Rule>) -> Result<MeteredAction, LowerError> {
        // pair is Rule::mint_token = { "mint_token" ~ block }
        let original_pair_span = pair.as_span(); // Get span before moving pair
        let block_pair = pair.into_inner().next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "mint_token missing block".to_string(),
                },
                original_pair_span, // Use original span
            )))
        })?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block in mint_token, found {:?}"#,
                        block_pair.as_rule()
                    ),
                },
                block_pair.as_span(),
            ))));
        }

        let block_pair_span = block_pair.as_span(); // Get span before moving block_pair
        let (_description, rules) = self.lower_block_common_fields(block_pair)?;

        let mut resource_type = String::new();
        let mut recipient: Option<String> = None;
        let mut amount: u64 = 1; // Default amount for minting
        let mut data: Option<Vec<DslRule>> = None;

        for rule in rules {
            match rule.key.as_str() {
                "type" => {
                    if let DslValue::String(s) = rule.value {
                        resource_type = s;
                    } else {
                        // Handle error or log: type should be a string
                    }
                }
                "recipient" | "recipients" => {
                    // Handle both singular and plural
                    if let DslValue::String(s) = rule.value {
                        // Assumes recipient is a string (identifier)
                        recipient = Some(s);
                    } else {
                        // Handle error or log: recipient should be a string
                    }
                }
                "amount" => {
                    if let DslValue::Number(n) = rule.value {
                        amount = n as u64; // Consider potential precision loss or error handling
                    } else {
                        // Handle error or log: amount should be a number
                    }
                }
                "data" => {
                    if let DslValue::Map(map_rules) = rule.value {
                        data = Some(map_rules);
                    } else {
                        // Handle error or log: data should be a map (block)
                    }
                }
                _ => { /* Ignore other fields for now */ }
            }
        }

        if resource_type.is_empty() {
            // It's an error if type is not specified for mint_token
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "mint_token requires a 'type' field".to_string(),
                },
                block_pair_span, // Use stored span
            ))));
        }

        Ok(MeteredAction {
            resource_type,
            amount,
            recipient,
            data,
        })
    }

    fn lower_anchor_data(&self, pair: Pair<'_, Rule>) -> Result<Anchor, LowerError> {
        // pair is Rule::anchor_data = { "anchor_data" ~ block }
        let original_pair_span = pair.as_span(); // Get span before moving pair
        let block_pair = pair.into_inner().next().ok_or_else(|| {
            LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "anchor_data missing block".to_string(),
                },
                original_pair_span, // Use original span
            )))
        })?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block in anchor_data, found {:?}"#,
                        block_pair.as_rule()
                    ),
                },
                block_pair.as_span(),
            ))));
        }

        let block_pair_span = block_pair.as_span(); // Get span before moving block_pair
        let (_description, rules) = self.lower_block_common_fields(block_pair)?;

        let mut data_reference = String::new();
        let mut path: Option<String> = None;

        for rule in rules {
            match rule.key.as_str() {
                "path" => {
                    if let DslValue::String(s) = rule.value {
                        path = Some(s);
                    } // else: path should be string, consider error/logging
                }
                "data" | "payload_cid" => {
                    match rule.value {
                        DslValue::String(s) => {
                            data_reference = s;
                        }
                        DslValue::Map(map_rules) => {
                            // For now, serialize the map to a placeholder string.
                            // In the future, this might involve hashing the content or a more structured representation.
                            data_reference = format!("map_content_placeholder_{:?}", map_rules);
                        }
                        // Handle other DslValue types if necessary, or error
                        _ => {
                            // Could set to a generic placeholder or error out
                            // For now, let's try to make a string representation to avoid panic
                            data_reference =
                                format!("unhandled_data_type_placeholder_{:?}", rule.value);
                        }
                    }
                }
                _ => { /* Ignore other fields */ }
            }
        }

        if data_reference.is_empty() {
            return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "anchor_data requires a 'data' or 'payload_cid' field that yields a reference string or map".to_string() },
                block_pair_span, // Use stored span
            ))));
        }

        Ok(Anchor {
            data_reference,
            path,
        })
    }

    fn lower_generic_section(&self, pair: Pair<'_, Rule>) -> Result<GenericSection, LowerError> {
        let kind_str_debug = format!("{:?}", pair.as_rule());
        // Example: kind_str_debug might be "OrganizationDef"
        // We want "organization" or "process", etc.
        let kind = kind_str_debug.to_lowercase().replace("_def", "");
        let original_pair_span = pair.as_span();

        let mut title: Option<String> = None;
        let mut block_pair_option: Option<Pair<'_, Rule>> = None;

        for inner_pair in pair.into_inner() {
            match inner_pair.as_rule() {
                Rule::string_literal => {
                    title = Some(inner_pair.as_str().trim_matches('"').to_string());
                }
                Rule::block => {
                    block_pair_option = Some(inner_pair);
                }
                _ => {
                    // This might happen if the grammar for a _def rule is more complex than expected
                    // or if a _def rule doesn't strictly follow string_literal? ~ block or just block.
                    return Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: format!(
                                "Unexpected rule {:?} inside generic section {}",
                                inner_pair.as_rule(),
                                kind
                            ),
                        },
                        inner_pair.as_span(),
                    ))));
                }
            }
        }

        if let Some(block_pair) = block_pair_option {
            let (_description, rules) = self.lower_block_common_fields(block_pair)?;
            Ok(GenericSection { kind, title, rules })
        } else {
            Err(LowerError::Parse(Box::new(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Generic section type '{}' missing main block"#, kind),
                },
                original_pair_span,
            ))))
        }
    }
}
