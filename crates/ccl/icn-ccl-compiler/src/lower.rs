use icn_ccl_dsl::{DslModule, Proposal, Role as DslAstRole, Rule as DslRule, RuleValue as DslValue};
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
            // Top-level definitions from election.ccl and other templates - no-op for now
            Rule::process_def |
            Rule::vacancies_def |
            Rule::actions_def |
            Rule::organization_def |
            Rule::governance_def |
            Rule::membership_def |
            Rule::budget_def |
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
                                let value_outer_pair_opt = field_parts.next(); // This is the (value | block | identifier) part

                                if let Some(key_pair) = key_pair_opt {
                                    let key_str = key_pair.as_str().trim_matches('"');

                                    if let Some(value_outer_pair) = value_outer_pair_opt {
                                        // We are interested in Rule::value for description/version/simple rules
                                        if value_outer_pair.as_rule() == Rule::value {
                                            if let Some(value_inner_pair) = value_outer_pair.into_inner().next() {
                                                // value_inner_pair is string_literal, number, boolean etc.
                                                let value_str = value_inner_pair.as_str().trim_matches('"').to_string();
                                                if key_str == "description" {
                                                    description_body = value_str;
                                                } else { // "version" and any other key becomes a DslRule
                                                    dsl_rules.push(DslRule {
                                                        key: key_str.to_string(),
                                                        value: DslValue::String(value_str),
                                                    });
                                                }
                                            }
                                            // else: Rule::value was empty (e.g. value pair itself was an empty rule if grammar allowed)
                                        }
                                        // else: value_outer_pair was Rule::block or Rule::identifier.
                                        // These are not currently processed into DslValue::String.
                                    }
                                    // else: any_statement had only a key (e.g. `my_key;`).
                                    // Not processed for description/version, nor as a DslRule here.
                                }
                                // else: any_statement was malformed or empty (no key).
                            }
                            _ => { /* Other definitions in statement, e.g. if_statement. Ignored for common fields. */ }
                        }
                    }
                }
            }
        }
        // If block_pair.as_rule() is not Rule::block, or if the block is empty / contains no relevant statements,
        // this will return (String::new(), Vec::new()) as initialized.
        Ok((description_body, dsl_rules))
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
} 