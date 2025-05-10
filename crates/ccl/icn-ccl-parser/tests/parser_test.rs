#[cfg(test)]
mod tests {
    use icn_ccl_parser::{CclParser, Rule};
    use insta::assert_snapshot;
    use pest::Parser;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_bylaws_template_parsing() {
        let template_path = "templates/bylaws.ccl";
        test_template_parsing(template_path);
    }

    #[test]
    fn test_budget_template_parsing() {
        let template_path = "templates/budget.ccl";
        test_template_parsing(template_path);
    }

    #[test]
    fn test_election_template_parsing() {
        let template_path = "templates/election.ccl";
        test_template_parsing(template_path);
    }

    fn test_template_parsing(template_path: &str) {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let full_path = manifest_dir.join(template_path);

        println!("Testing template: {}", full_path.display());

        let content = fs::read_to_string(&full_path)
            .unwrap_or_else(|_| panic!("Failed to read template file: {}", full_path.display()));

        // Test basic parsing with Pest
        let parsed_result = CclParser::parse(Rule::ccl, &content);
        assert!(
            parsed_result.is_ok(),
            "Failed to parse: {:?}",
            parsed_result.err()
        );

        let successful_parse = parsed_result.unwrap(); // Unwrap once

        // Snapshot testing for the parse tree
        let pairs_for_snapshot = successful_parse.clone().collect::<Vec<_>>(); // Clone Pairs for snapshotting

        let snapshot_name = Path::new(template_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown_template");

        let pairs_debug_string = format!("{:#?}", pairs_for_snapshot);
        assert_snapshot!(snapshot_name, pairs_debug_string);

        // Count the number of rules matched
        println!("Successfully parsed {} rules", pairs_for_snapshot.len());

        // Try to find some specific constructs
        let constructs = ["mint_token", "anchor_data", "perform_metered_action"];

        for construct in constructs {
            assert!(
                content.contains(construct),
                "Template should contain '{}' construct",
                construct
            );
        }

        // Verify the document can convert to DSL (even if placeholder)
        // let document = CclDocument::parse(&content).expect("Failed to parse into document");
        // let dsl = document.to_dsl().expect("Failed to convert to DSL");
        // assert!(!dsl.is_empty(), "DSL output should not be empty");

        println!("✓ Template '{}' passed verification", template_path);
    }
}
