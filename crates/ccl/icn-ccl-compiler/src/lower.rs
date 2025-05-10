use icn_ccl_dsl::{
    ActionHandler, ActionStep, Anchor, DslModule, MeteredAction, Proposal, Role as DslAstRole,
    Rule as DslRule, RuleValue as DslValue, RangeRule,
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
    Parse(#[from] pest::error::Error<Rule>),
    #[error("unhandled rule: {0:?}")]
    Unhandled(Pair<'static, Rule>),
}

/// Primary entry‐point used by CLI & tests.
pub fn lower_str(src: &str) -> Result<Vec<DslModule>, LowerError> {
    let mut pairs = CclParser::parse(Rule::ccl, src)?;
    let ccl_root_pair = pairs.next().ok_or_else(|| {
        // This case should ideally not happen if parsing Rule::ccl was successful
        // and the grammar expects at least SOI/EOI or some content.
        // Creating a generic parse error if it does.
        pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError {
                message: "Expected a CCL root pair but found none.".to_string(),
            },
            pest::Span::new(src, 0, 0).unwrap(), // Dummy span
        )
    })?;
    Lowerer::default().lower(ccl_root_pair.into_inner())
}

#[derive(Default)]
struct Lowerer;

impl Lowerer {
    fn lower(&self, pairs: Pairs<'_, Rule>) -> Result<Vec<DslModule>, LowerError> {
        let mut modules = Vec::new();

        for pair in pairs {
            match pair.as_rule() {
                Rule::statement => {
                    // statement -> inner definition(s)
                    for inner in pair.into_inner() {
                        // Pass a mutable reference to modules
                        self.dispatch_def(&mut modules, inner)?;
                    }
                }
                // Handle cases where a definition might not be wrapped in a statement,
                // or for rules like EOI that are direct children of ccl_root_pair.into_inner()
                _ => self.dispatch_def(&mut modules, pair)?,
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
                        return Err(LowerError::Parse(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError {
                                message: format!("Expected block within roles_def, found {:?}", block_pair.as_rule()),
                            },
                            block_pair.as_span(),
                        )));
                    }
                } else {
                    // roles_def was empty or did not contain a block, also an error.
                    return Err(LowerError::Parse(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError {
                            message: "roles_def is empty or missing a block".to_string(),
                        },
                        pair_span, // Use the stored span
                    )));
                }
            }
            Rule::actions_def => {
                modules.extend(self.lower_actions(pair)?);
            }
            // Top-level definitions from election.ccl and other templates - no-op for now
            Rule::process_def |
            Rule::vacancies_def |
            Rule::organization_def |
            Rule::governance_def |
            Rule::membership_def |
            Rule::allocations_def |
            Rule::spending_rules_def |
            Rule::reporting_def => {
                // TODO: Implement lowering for these rules
                // For now, consuming the pair and doing nothing allows tests to proceed
            }

            Rule::EOI => {} // EOI will be the last item from ccl_root_pair.into_inner()
            _other => {
                // Avoid transmuting a Pair that might be 'static if it came from the top level
                // For now, let's create a new owned Pair for the error if it's not proposal_def or EOI/SOI
                // A better approach would be to ensure all handled rules are exhaustive or make Unhandled take Pair<'_, Rule>
                // but that requires changing LowerError and its usage.
                // For simplicity in this step, we create an owned string representation for the error.
                // This is a simplification; a robust solution would handle lifetimes carefully.
                return Err(LowerError::Unhandled(unsafe { std::mem::transmute(pair) }));
            }
        }
        Ok(())
    }

    fn lower_roles_from_block(&self, block_pair: Pair<'_, Rule>, modules: &mut Vec<DslModule>) -> Result<(), LowerError> {
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

    fn lower_single_role_def(&self, role_def_pair: Pair<'_, Rule>) -> Result<DslAstRole, LowerError> {
        // role_def = { "role" ~ string_literal ~ block }
        let pair_span = role_def_pair.as_span(); // Span of the whole role_def for error reporting
        let mut inner_role_pairs = role_def_pair.into_inner();

        // First inner is string_literal (role name)
        let role_name_pair = inner_role_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Role definition missing name".to_string() },
            pair_span, // Error points to the whole role_def
        )))?;
        // Ensure it's a string_literal as expected by grammar `role_def = { "role" ~ string_literal ~ block }`
        // Note: role_name_pair.as_rule() might be Rule::inner_string if string_literal is silent.
        // The grammar for string_literal is `${ """ ~ inner_string ~ """ }`.
        // inner_string is `@ { ... }`. `as_str()` on `string_literal` includes quotes.
        let role_name = role_name_pair.as_str().trim_matches('"').to_string();

        // Second inner is block (role attributes)
        let role_block_pair = inner_role_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Role definition for '{}' missing block", role_name) },
            pair_span, // Error points to the whole role_def
        )))?;

        if role_block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!("Expected block for role '{}', found {:?}", role_name, role_block_pair.as_rule()),
                },
                role_block_pair.as_span(),
            )));
        }

        let (description, attributes) = self.lower_block_common_fields(role_block_pair)?;

        Ok(DslAstRole {
            name: role_name,
            description: if description.is_empty() { None } else { Some(description) },
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
                                let mut field_parts = inner_def_pair.into_inner();
                                let key_pair_opt = field_parts.next();
                                let value_outer_pair_opt = field_parts.next();

                                if let Some(key_pair) = key_pair_opt {
                                    let key_str = key_pair.as_str().trim_matches('"');

                                    if let Some(value_outer_pair) = value_outer_pair_opt {
                                        // value_outer_pair is the (value | block | identifier) part of any_statement
                                        match value_outer_pair.as_rule() {
                                            Rule::value => {
                                                if let Some(value_inner_pair) = value_outer_pair.into_inner().next() {
                                                    let dsl_val = self.lower_value_rule(value_inner_pair)?;
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
                                                // Recursive call to process the nested block
                                                let (_nested_desc, nested_rules) = self.lower_block_common_fields(value_outer_pair)?;
                                                // If the nested block produced a description, it's ignored here.
                                                // The key_str gets a DslValue::Map of the rules from the nested block.
                                                dsl_rules.push(DslRule {
                                                    key: key_str.to_string(),
                                                    value: DslValue::Map(nested_rules),
                                                });
                                            }
                                            Rule::general_identifier => {
                                                // Treat general_identifier as a string value for the rule
                                                dsl_rules.push(DslRule {
                                                    key: key_str.to_string(),
                                                    value: DslValue::String(value_outer_pair.as_str().to_string()),
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
                                let key = format!("range_{}_{}", range_rule_data.start, range_rule_data.end);
                                dsl_rules.push(DslRule {
                                    key,
                                    value: DslValue::Range(Box::new(range_rule_data)),
                                });
                            }
                            _ => { /* Other definitions in statement. Ignored for common fields. */ }
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
            Rule::string_literal => Ok(DslValue::String(value_pair.as_str().trim_matches('"').to_string())),
            Rule::number => {
                let num_str = value_pair.as_str();
                num_str.parse::<f64>().map(DslValue::Number)
                    .map_err(|e| LowerError::Parse(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: format!("Invalid number: {}", e) },
                        value_pair.as_span(),
                    )))
            }
            Rule::boolean => {
                value_pair.as_str().parse::<bool>().map(DslValue::Boolean)
                    .map_err(|e| LowerError::Parse(pest::error::Error::new_from_span(
                        pest::error::ErrorVariant::CustomError { message: format!("Invalid boolean: {}", e) },
                        value_pair.as_span(),
                    )))
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
            // TODO: Rule::object, Rule::general_identifier (if not handled as string above), Rule::function_call
            _ => Ok(DslValue::String(format!("UNPROCESSED_VALUE_RULE_{:?}_{}", value_pair.as_rule(), value_pair.as_str()))), // Placeholder
        }
    }

    fn lower_range_statement(&self, pair: Pair<'_, Rule>) -> Result<RangeRule, LowerError> {
        // pair is Rule::range_statement = { "range" ~ number ~ number ~ block }
        let original_span = pair.as_span(); // For top-level error reporting
        let mut inner_pairs = pair.into_inner();

        let start_pair = inner_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Range statement missing start number".to_string() },
            original_span,
        )))?;
        let start_val = start_pair.as_str().parse::<f64>().map_err(|e| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Invalid start number for range: {}", e) },
            start_pair.as_span(),
        )))?;

        let end_pair = inner_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Range statement missing end number".to_string() },
            original_span,
        )))?;
        let end_val = end_pair.as_str().parse::<f64>().map_err(|e| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: format!("Invalid end number for range: {}", e) },
            end_pair.as_span(),
        )))?;

        let block_pair = inner_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Range statement missing block".to_string() },
            original_span,
        )))?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: format!("Expected block in range statement, found {:?}", block_pair.as_rule()) },
                block_pair.as_span(),
            )));
        }

        // The description part from lower_block_common_fields is not used for RangeRule's sub-rules.
        let (_description, rules_for_range) = self.lower_block_common_fields(block_pair)?;

        Ok(RangeRule {
            start: start_val,
            end: end_val,
            rules: rules_for_range,
        })
    }

    fn lower_proposal(&self, pair: Pair<'_, Rule>) -> Result<Proposal, LowerError> {
        let pair_span = pair.as_span();
        let mut proposal_specific_pairs = pair.into_inner(); // These are specific to proposal_def

        let title = proposal_specific_pairs
            .next()
            .ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Proposal missing title".to_string() },
                pair_span,
            )))?
            .as_str()
            .trim_matches('"')
            .to_owned();

        let block_pair = proposal_specific_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Proposal missing block".to_string() },
            pair_span,
        )))?;

        let (description_body, dsl_rules) = self.lower_block_common_fields(block_pair)?;
        
        Ok(self.build_stub_proposal(title, description_body, dsl_rules))
    }

    fn lower_election(&self, pair: Pair<'_, Rule>) -> Result<Proposal, LowerError> {
        let pair_span = pair.as_span();
        let mut election_specific_pairs = pair.into_inner(); // These are specific to election_def

        let title = election_specific_pairs
            .next()
            .ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError { message: "Election missing title".to_string() },
                pair_span,
            )))?
            .as_str()
            .trim_matches('"')
            .to_owned();

        let block_pair = election_specific_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Election missing block".to_string() },
            pair_span, 
        )))?;

        let (description_body, dsl_rules) = self.lower_block_common_fields(block_pair)?;

        Ok(self.build_stub_proposal(title, description_body, dsl_rules))
    }

    fn build_stub_proposal(&self, title: String, body: String, rules: Vec<DslRule>) -> Proposal {
        let id = {
            #[cfg(test)]
            { Uuid::parse_str(TEST_UUID_STR).unwrap() } 
            #[cfg(not(test))]
            { Uuid::new_v4() } 
        };

        Proposal {
            id,
            title,
            body,
            author: "unknown".into(),
            created_at: 0,
            rules, // Use passed in rules
        }
    }

    fn lower_actions(&self, pair: Pair<'_, Rule>) -> Result<Vec<DslModule>, LowerError> {
        let mut handlers = Vec::new();
        let pair_span = pair.as_span();
        let block_pair = pair.into_inner().next().ok_or_else(|| {
            LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: "actions_def is missing a block".to_string(),
                },
                pair_span, // Use stored span
            ))
        })?;

        if block_pair.as_rule() != Rule::block {
            return Err(LowerError::Parse(pest::error::Error::new_from_span(
                pest::error::ErrorVariant::CustomError {
                    message: format!(
                        "Expected block within actions_def, found {:?}",
                        block_pair.as_rule()
                    ),
                },
                block_pair.as_span(),
            )));
        }

        for statement_pair_outer in block_pair.into_inner() { // Iterates statements in actions block
            let outer_rule = statement_pair_outer.as_rule();
            let _outer_span = statement_pair_outer.as_span(); // Used only in commented-out eprintln
            // eprintln!("[lower_actions] Processing statement_pair_outer: {:?}, rule: {:?}", _outer_span, outer_rule);

            if outer_rule == Rule::statement {
                if let Some(on_pair) = statement_pair_outer.into_inner().next() { 
                    let on_pair_rule = on_pair.as_rule();
                    let _on_pair_span_for_log = on_pair.as_span(); // Used only in commented-out eprintln
                    if on_pair_rule == Rule::action_def {
                        let _on_pair_span = on_pair.as_span(); // Renamed from on_pair_span, not used after eprintln removal
                        let mut inner_action_def_pairs = on_pair.into_inner();
                        let event_name_pair = inner_action_def_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError { message: "action_def missing event name (string_literal)".to_string() },
                            _on_pair_span, // Use stored span
                        )))?;
                        let event = event_name_pair.as_str().trim_matches('"').to_owned();

                        let steps_block_pair = inner_action_def_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
                            pest::error::ErrorVariant::CustomError { message: format!("action_def for event '{}' missing block", event) },
                            _on_pair_span, // Use stored span
                        )))?;

                        if steps_block_pair.as_rule() != Rule::block {
                             return Err(LowerError::Parse(pest::error::Error::new_from_span(
                                pest::error::ErrorVariant::CustomError {
                                    message: format!("Expected block for action_def event '{}', found {:?}", event, steps_block_pair.as_rule()),
                                },
                                steps_block_pair.as_span(),
                            )));
                        }

                        let mut steps = Vec::new();
                        for step_statement_pair_outer in steps_block_pair.into_inner() { 
                            let step_outer_rule = step_statement_pair_outer.as_rule();
                            let _step_outer_span = step_statement_pair_outer.as_span(); // Used only in commented-out eprintln
                            // eprintln!("[lower_actions] Processing step_statement_pair_outer: {:?}, rule: {:?}", _step_outer_span, step_outer_rule);

                            if step_outer_rule == Rule::statement {
                                if let Some(step_pair) = step_statement_pair_outer.into_inner().next() { 
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
                                        }
                                        _ => {
                                        }
                                    }
                                }
                            }
                        }
                        handlers.push(DslModule::ActionHandler(ActionHandler { event, steps }));
                    } else {
                        // eprintln!("[lower_actions] Expected Rule::action_def, got: {:?}. Span: {:?}", on_pair_rule, _on_pair_span_for_log);
                    }
                } else {
                     // eprintln!("[lower_actions] statement_pair_outer (rule: {:?}) had no inner on_pair. Span: {:?}", outer_rule, _outer_span);
                }
            } else {
                // eprintln!("[lower_actions] Expected Rule::statement for on_pair container, got: {:?}. Span: {:?}", outer_rule, _outer_span);
            }
        }
        Ok(handlers)
    }

    fn lower_mint_token(&self, _pair: Pair<'_, Rule>) -> Result<MeteredAction, LowerError> {
        // _pair is Rule::mint_token = { "mint_token" ~ block }
        // For MVP, just returning placeholder.
        Ok(MeteredAction {
            resource_type: "token".into(), // Placeholder
            amount: 0,                     // Placeholder, TODO: parse from block
        })
    }

    fn lower_anchor_data(&self, _pair: Pair<'_, Rule>) -> Result<Anchor, LowerError> {
        // _pair is Rule::anchor_data = { "anchor_data" ~ block }
        // For MVP, just returning placeholder.
        Ok(Anchor {
            // The Anchor struct in DSL only has payload_cid
            payload_cid: "bafy...placeholder".into(), // Placeholder, TODO: parse from block
        })
    }
} 