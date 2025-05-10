use clap::{Arg, ArgAction, ArgMatches, Command};
use icn_ccl_parser::{CclDocument, CclError, CclParserResult};

fn main() -> CclParserResult<()> {
    let matches = Command::new("ccl-parser")
        .version("0.1.0")
        .author("ICN Todaro")
        .about("Parses and validates CCL (Cooperative Computing Language) files")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("parse")
                .about("Parses a CCL file and prints its structure")
                .arg(Arg::new("input").required(true).help("Input CCL file"))
                .arg(
                    Arg::new("print-dsl")
                        .long("print-dsl")
                        .action(ArgAction::SetTrue)
                        .help("Prints the generated DSL code"),
                ),
        )
        .subcommand(
            Command::new("validate")
                .about("Validates a CCL file")
                .arg(Arg::new("input").required(true).help("Input CCL file")),
        )
        .subcommand(
            Command::new("compile")
                .about("Compiles a CCL file to WASM (stubbed)")
                .arg(Arg::new("input").required(true).help("Input CCL file"))
                .arg(
                    Arg::new("output")
                        .short('o')
                        .long("output")
                        .help("Output WASM file path"),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("parse", sub_matches)) => parse_ccl_command(sub_matches)?,
        Some(("validate", sub_matches)) => validate_ccl_command(sub_matches)?,
        Some(("compile", sub_matches)) => compile_ccl_to_wasm_command(sub_matches)?,
        _ => unreachable!("Exhausted list of subcommands and subcommand_required prevents `None`"),
    }
    Ok(())
}

fn parse_ccl_command(matches: &ArgMatches) -> CclParserResult<()> {
    let input_path_str = matches
        .get_one::<String>("input")
        .ok_or_else(|| CclError::InvalidInput("Missing input file path".to_string()))?;
    let file_content = std::fs::read_to_string(input_path_str).map_err(CclError::IoError)?;
    let document = CclDocument::parse(&file_content)
        .map_err(|e| CclError::ParseError(format!("Failed to parse CCL: {}", e)))?;

    println!("Successfully parsed CCL document:");
    println!("Title: {}", document.title);

    if matches.get_flag("print-dsl") {
        match document.to_dsl() {
            Ok(dsl) => {
                println!("\nGenerated DSL:");
                println!("{}", dsl);
            }
            Err(e) => {
                eprintln!("\nError generating DSL: {}", e);
            }
        }
    }
    Ok(())
}

fn validate_ccl_command(matches: &ArgMatches) -> CclParserResult<()> {
    let input_path_str = matches
        .get_one::<String>("input")
        .ok_or_else(|| CclError::InvalidInput("Missing input file path".to_string()))?;
    let file_content = std::fs::read_to_string(input_path_str).map_err(CclError::IoError)?;
    CclDocument::parse(&file_content)
        .map_err(|e| CclError::ValidationError(format!("CCL validation failed: {}", e)))?;
    println!("CCL file is valid (basic check).");
    Ok(())
}

fn compile_ccl_to_wasm_command(_matches: &ArgMatches) -> CclParserResult<()> {
    println!("Compilation to WASM is handled by icn-ccl-compiler. This utility (ccl-parser) might be deprecated or used for parsing checks only.");
    Ok(())
}
