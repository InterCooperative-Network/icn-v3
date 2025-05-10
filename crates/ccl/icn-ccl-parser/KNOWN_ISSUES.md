# Known Parsing Issues in CCL Parser

This document tracks known parsing issues and ambiguities in the `ccl.pest` grammar that need further investigation and resolution.

## 1. Ambiguity with `identifier ~ value` Statements without Semicolons

**Affected File(s):** `crates/ccl/icn-ccl-parser/templates/election.ccl` (and potentially others)

**Symptom:**
Lines matching an `identifier ~ <value>` pattern (e.g., `term_limit 2`, `seats 7`, `skills ["finance", "accounting"]`, `max_statement_length 2000`, `transition_period 14d`) fail to parse correctly if they are:
    a) Not explicitly terminated with a semicolon (`;`).
    b) Followed by a line comment (`// ...`).
    c) Potentially when they are the last statement in a block before the closing `}`.

The parser often expects a `block` or a new `statement` instead of correctly parsing these as simple key-value assignments handled by the `any_statement` rule (or a more specific rule if one were to be matched).

**Current Workaround:**
The problematic lines in `election.ccl` have been temporarily commented out to allow tests to pass. They are marked with `// FIXME: CCL parsing issue`.

**Example (from `election.ccl`):**
```ccl
role "steward" {
  description "Governance steward with administrative capabilities"
  term_length 365d // 1 year term
  // term_limit 2 // Maximum consecutive terms // FIXME: CCL parsing issue
  // seats 7 // Number of stewards // FIXME: CCL parsing issue
  requirements {
    membership_duration 365d // Must be a member for 1 year
    standing "good"
    // skills ["finance", "accounting"] // FIXME: CCL parsing issue
  }
}

// ... and other similar instances ...
```

**Suspected Cause:**
The issue likely stems from an ambiguity in the grammar where the `WHITESPACE` rule (which consumes comments) interacts with statement termination. The optional nature of semicolons for `any_statement` might be too greedy or not correctly prioritized when a comment follows on the same line, or when a block ends. The Pest parser might be backtracking or failing to match `any_statement` in these specific contexts due to how rules are ordered or defined, particularly concerning `block` and `NEWLINE` interactions within `any_statement` or the main `statement` rule.

**Next Steps:**
*   Revisit the `any_statement`, `statement`, `block`, and `WHITESPACE` rules in `ccl.pest`.
*   Experiment with making semicolons mandatory for such simple assignments if ambiguity cannot be resolved otherwise.
*   Investigate Pest's parsing order and backtracking behavior for these specific cases.
*   Consider adding more specific rules for `identifier ~ number`, `identifier ~ duration`, `identifier ~ array` if `any_statement` proves too problematic, ensuring they are correctly prioritized.
*   Uncomment the lines in `election.ccl` and verify the fix. Update snapshot tests accordingly.

This issue should be addressed after the main CCL -> DSL -> WASM pipeline is functional to avoid blocking core progress. 