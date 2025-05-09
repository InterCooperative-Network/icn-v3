use icn_ccl_parser::{CclDocument, CclError};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), CclError> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Usage: {} <command> [args...]", args[0]);
        println!("Commands:");
        println!("  parse <file.ccl>               - Parse a CCL file");
        println!("  compile <file.ccl> <file.dsl>  - Compile CCL to DSL");
        println!("  verify <file.ccl>              - Verify a CCL file is valid");
        return Ok(());
    }

    match args[1].as_str() {
        "parse" => {
            if args.len() < 3 {
                return Err(CclError::InvalidStructure("Missing input file".to_string()));
            }

            let file_path = PathBuf::from(&args[2]);
            let content = fs::read_to_string(&file_path)?;

            let document = CclDocument::parse(&content)?;
            println!(
                "Successfully parsed CCL document with {} statements",
                document.statements.len()
            );

            // Output some basic statistics
            let mut statement_types = std::collections::HashMap::new();
            for statement in &document.statements {
                let type_name = match statement {
                    icn_ccl_parser::CclStatement::Organization { .. } => "Organization",
                    icn_ccl_parser::CclStatement::Role { .. } => "Role",
                    icn_ccl_parser::CclStatement::Governance { .. } => "Governance",
                    icn_ccl_parser::CclStatement::Action { .. } => "Action",
                    icn_ccl_parser::CclStatement::Budget { .. } => "Budget",
                    icn_ccl_parser::CclStatement::Election { .. } => "Election",
                    icn_ccl_parser::CclStatement::Custom { .. } => "Custom",
                };

                *statement_types.entry(type_name).or_insert(0) += 1;
            }

            println!("Statement types:");
            for (k, v) in statement_types {
                println!("  {}: {}", k, v);
            }
        }
        "compile" => {
            if args.len() < 4 {
                return Err(CclError::InvalidStructure(
                    "Missing input or output file".to_string(),
                ));
            }

            let input_path = PathBuf::from(&args[2]);
            let output_path = PathBuf::from(&args[3]);

            let content = fs::read_to_string(&input_path)?;
            let document = CclDocument::parse(&content)?;

            let dsl = document.to_dsl()?;
            fs::write(&output_path, dsl)?;

            println!(
                "Successfully compiled {} to DSL format",
                input_path.display()
            );
        }
        "verify" => {
            if args.len() < 3 {
                return Err(CclError::InvalidStructure("Missing input file".to_string()));
            }

            let file_path = PathBuf::from(&args[2]);
            let content = fs::read_to_string(&file_path)?;

            let document = CclDocument::parse(&content)?;
            document.verify()?;

            println!("CCL document {} is valid", file_path.display());
        }
        _ => {
            return Err(CclError::InvalidStructure(format!(
                "Unknown command: {}",
                args[1]
            )));
        }
    }

    Ok(())
}
