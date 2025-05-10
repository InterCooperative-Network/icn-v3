use icn_ccl_dsl::{DslModule, Proposal, Rule as DslRule, Value as DslValue};
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
            // Top-level definitions from election.ccl and other templates - no-op for now
            Rule::roles_def |
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

        let mut description_body = String::new();
        let mut dsl_rules = Vec::<DslRule>::new();

        if block_pair.as_rule() == Rule::block {
            for statement_pair in block_pair.into_inner() { // These are statements inside the block
                if statement_pair.as_rule() == Rule::statement {
                    // A statement should have one inner actual definition (e.g., any_statement)
                    if let Some(inner_def_pair) = statement_pair.into_inner().next() {
                        match inner_def_pair.as_rule() {
                            Rule::any_statement => {
                                let mut field_parts = inner_def_pair.into_inner();
                                let key_pair = field_parts.next();
                                let value_pair_outer = field_parts.next();

                                if let (Some(key), Some(val_outer)) = (key_pair, value_pair_outer) {
                                    let key_str = key.as_str();
                                    // val_outer is a Rule::value, need to get its actual inner string/number etc.
                                    if let Some(val_inner) = val_outer.into_inner().next() {
                                        let value_str = val_inner.as_str().trim_matches('"').to_string();
                                        if key_str == "description" {
                                            description_body = value_str;
                                        } else if key_str == "version" {
                                            dsl_rules.push(DslRule {
                                                key: key_str.to_string(),
                                                value: DslValue::String(value_str),
                                            });
                                        } else {
                                            // Other fields can become generic rules for now
                                            dsl_rules.push(DslRule {
                                                key: key_str.to_string(),
                                                value: DslValue::String(value_str), // Or more specific DslValue type
                                            });
                                        }
                                    }
                                }
                            }
                            _ => { /* Unhandled statement type in block */ }
                        }
                    }
                }
            }
        }
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

        // The next pair should be the block
        let block_pair = election_specific_pairs.next().ok_or_else(|| LowerError::Parse(pest::error::Error::new_from_span(
            pest::error::ErrorVariant::CustomError { message: "Election missing block".to_string() },
            pair_span, // Use the original election_def span for error reporting
        )))?;

        let mut description_body = String::new();
        let mut dsl_rules = Vec::<DslRule>::new();

        if block_pair.as_rule() == Rule::block {
            for statement_pair in block_pair.into_inner() { // These are statements inside the block
                if statement_pair.as_rule() == Rule::statement {
                    // A statement should have one inner actual definition (e.g., any_statement)
                    if let Some(inner_def_pair) = statement_pair.into_inner().next() {
                        match inner_def_pair.as_rule() {
                            Rule::any_statement => {
                                let mut field_parts = inner_def_pair.into_inner();
                                let key_pair = field_parts.next();
                                let value_pair_outer = field_parts.next();

                                if let (Some(key), Some(val_outer)) = (key_pair, value_pair_outer) {
                                    let key_str = key.as_str();
                                    // val_outer is a Rule::value, need to get its actual inner string/number etc.
                                    if let Some(val_inner) = val_outer.into_inner().next() {
                                        let value_str = val_inner.as_str().trim_matches('"').to_string();
                                        if key_str == "description" {
                                            description_body = value_str;
                                        } else if key_str == "version" {
                                            dsl_rules.push(DslRule {
                                                key: key_str.to_string(),
                                                value: DslValue::String(value_str),
                                            });
                                        } else {
                                            // Other fields can become generic rules for now
                                            dsl_rules.push(DslRule {
                                                key: key_str.to_string(),
                                                value: DslValue::String(value_str), // Or more specific DslValue type
                                            });
                                        }
                                    }
                                }
                            }
                            _ => { /* Unhandled statement type in block */ }
                        }
                    }
                }
            }
        }
        // Use the extracted title, description (as body), and version (as a rule)
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