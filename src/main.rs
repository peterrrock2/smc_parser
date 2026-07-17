use ben::encode::BenEncoder;
use clap::{Arg, ArgAction, Command};
use serde_json::{json, Map, Value};
use std::{
    env,
    fs::{self, File},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

const CONFIG_VERSION: i64 = 1;
const IO_FIELDS: &[&str] = &["graph", "output", "writer"];
const MAP_FIELDS: &[&str] = &["pop_col", "n_dists", "pop_tol", "pop_bounds"];
const RUN_FIELDS: &[&str] = &[
    "n_sims",
    "rng_seed",
    "compactness",
    "resample",
    "adapt_k_thresh",
    "seq_alpha",
    "pop_temper",
    "final_infl",
    "est_label_mult",
    "verbose",
    "silent",
    "tally_columns",
];

macro_rules! log {
    ($($arg:tt)*) => {{
        if let Ok(log_level) = std::env::var("RUST_LOG") {
            if log_level.to_lowercase() == "trace" {
                eprint!($($arg)*);
            }
        }
    }};
}

macro_rules! logln {
    ($($arg:tt)*) => {{
        if let Ok(log_level) = std::env::var("RUST_LOG") {
            if log_level.to_lowercase() == "trace" {
                eprintln!($($arg)*);
            }
        }
    }};
}

#[derive(Debug, PartialEq)]
struct RunConfig {
    raw: String,
    output: Option<String>,
    writer: String,
}

fn require_object<'a>(
    value: &'a Value,
    label: &str,
) -> std::result::Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be a JSON object"))
}

fn require_fields(
    value: &Map<String, Value>,
    required: &[&str],
    label: &str,
) -> std::result::Result<(), String> {
    let missing = required
        .iter()
        .filter(|field| !value.contains_key(**field))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label} is missing required fields: {}",
            missing.join(", ")
        ))
    }
}

fn parse_config(raw: &str) -> std::result::Result<RunConfig, String> {
    let config: Value =
        serde_json::from_str(raw).map_err(|error| format!("Invalid config JSON: {error}"))?;
    let config = require_object(&config, "config")?;
    require_fields(
        config,
        &["version", "engine", "io", "map", "run", "constraints"],
        "config",
    )?;

    let version = config["version"]
        .as_i64()
        .ok_or_else(|| "config.version must be an integer".to_string())?;
    if version != CONFIG_VERSION {
        return Err(format!("Unsupported config version {version}"));
    }
    if config["engine"].as_str() != Some("smc") {
        return Err(format!("Expected engine 'smc', got '{}'", config["engine"]));
    }

    let io_config = require_object(&config["io"], "config.io")?;
    let map_config = require_object(&config["map"], "config.map")?;
    let run_config = require_object(&config["run"], "config.run")?;
    require_fields(io_config, IO_FIELDS, "config.io")?;
    require_fields(map_config, MAP_FIELDS, "config.map")?;
    require_fields(run_config, RUN_FIELDS, "config.run")?;
    if !config["constraints"].is_array() {
        return Err("config.constraints must be a JSON array".to_string());
    }
    if !io_config["graph"].is_string() {
        return Err("config.io.graph must be a string".to_string());
    }

    let writer = io_config["writer"]
        .as_str()
        .ok_or_else(|| "config.io.writer must be a string".to_string())?
        .to_string();
    if writer != "jsonl" && writer != "ben" {
        return Err(format!("Unsupported smc_parser writer '{writer}'"));
    }
    let output = match &io_config["output"] {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        _ => return Err("config.io.output must be a string or null".to_string()),
    };

    Ok(RunConfig {
        raw: raw.to_string(),
        output,
        writer,
    })
}

fn load_config_env(environment_name: &str) -> std::result::Result<RunConfig, String> {
    let raw = env::var(environment_name)
        .map_err(|_| format!("Environment variable {environment_name} is not set"))?;
    parse_config(&raw)
}

fn metadata_path(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .expect("output path must have a file name")
        .to_string_lossy();
    output_path.with_file_name(format!("{stem}_metadata.jsonl"))
}

fn write_metadata(output_path: &Path, raw_config: &str) -> io::Result<()> {
    fs::write(metadata_path(output_path), format!("{raw_config}\n"))
}

fn canonicalize_jsonl_from_print<R: BufRead, W: Write>(reader: R, mut writer: W) -> io::Result<()> {
    let mut start = false;
    let mut sample = 1;

    for line in reader.lines() {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if !start && str_line == "Now printing the plans:" {
            start = true;
            continue;
        }
        if !start {
            continue;
        }

        log!("Processing sample {sample}\r");
        let assignment = serde_json::from_str::<Vec<u16>>(&str_line).unwrap_or_else(|_| {
            panic!("Failed to parse line {sample} with value {str_line} as Vec<u16>")
        });
        writeln!(
            writer,
            "{}",
            json!({"assignment": assignment, "sample": sample})
        )?;
        sample += 1;
    }
    if !start {
        panic!(concat!(
            "ERROR: Could not find the start of the plans in the print output. ",
            "Please make sure the input is a print output of SMC."
        ));
    }
    logln!();
    logln!("Done!");
    Ok(())
}

fn canonicalize_jsonl_from_csv<R: BufRead, W: Write>(reader: R, mut writer: W) -> io::Result<()> {
    let mut csv_reader = csv::Reader::from_reader(reader);

    for (sample, line) in (1..).zip(csv_reader.records()) {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        log!("Processing sample {sample}\r");
        let assignment = str_line
            .iter()
            .skip(1)
            .map(|value| {
                value
                    .parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse {value} as u16 in sample {sample}"))
            })
            .collect::<Vec<u16>>();
        writeln!(
            writer,
            "{}",
            json!({"assignment": assignment, "sample": sample})
        )?;
    }

    logln!();
    logln!("Done!");
    Ok(())
}

fn canonicalize_ben_from_print<R: BufRead, W: Write>(reader: R, writer: W) -> io::Result<()> {
    let mut start = false;
    let mut sample = 1;
    let mut ben_encoder = BenEncoder::new(writer);

    for line in reader.lines() {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if !start && str_line == "Now printing the plans:" {
            start = true;
            continue;
        }
        if !start {
            continue;
        }

        log!("Processing sample {sample}\r");
        let assignment = serde_json::from_str::<Vec<u16>>(&str_line).unwrap_or_else(|_| {
            panic!("Failed to parse line {sample} with value {str_line} as Vec<u16>")
        });
        ben_encoder.write_assignment(assignment)?;
        sample += 1;
    }
    if !start {
        panic!(concat!(
            "ERROR: Could not find the start of the plans in the print output. ",
            "Please make sure the input is a print output of SMC."
        ));
    }
    logln!();
    logln!("Done!");
    Ok(())
}

fn canonicalize_ben_from_csv<R: BufRead, W: Write>(reader: R, writer: W) -> io::Result<()> {
    let mut csv_reader = csv::Reader::from_reader(reader);
    let mut ben_encoder = BenEncoder::new(writer);

    for (sample, line) in (1..).zip(csv_reader.records()) {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        log!("Processing sample {sample}\r");
        let assignment = str_line
            .iter()
            .skip(1)
            .map(|value| {
                value
                    .parse::<u16>()
                    .unwrap_or_else(|_| panic!("Failed to parse {value} as u16 in sample {sample}"))
            })
            .collect::<Vec<u16>>();
        ben_encoder.write_assignment(assignment)?;
    }
    logln!();
    logln!("Done!");
    Ok(())
}

fn cli() -> Command {
    Command::new("smc-parser")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Canonicalize redist SMC output")
        .arg(
            Arg::new("config_env")
                .long("config-env")
                .help("Read a versioned gerrytools config from this environment variable")
                .conflicts_with_all([
                    "input_csv",
                    "output_file",
                    "jsonl",
                    "ben",
                    "verbose",
                    "overwrite",
                ]),
        )
        .arg(
            Arg::new("input_csv")
                .short('i')
                .long("input-csv")
                .help("Path to the input CSV file"),
        )
        .arg(
            Arg::new("output_file")
                .short('o')
                .long("output-file")
                .help("Path to the output file"),
        )
        .arg(
            Arg::new("jsonl")
                .short('j')
                .long("jsonl")
                .help("Convert to JSONL")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("ben")
                .short('b')
                .long("ben")
                .help("Convert to BEN")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Verbose output")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("overwrite")
                .short('w')
                .long("overwrite")
                .help("Overwrite output file")
                .action(ArgAction::SetTrue),
        )
}

fn prompt_before_overwrite(path: &Path) {
    eprint!("File {path:?} already exists. Would you like to overwrite? y/[n]: ");
    let mut response = String::new();
    io::stdin()
        .read_line(&mut response)
        .expect("Error reading response");
    if response.trim() != "y" {
        std::process::exit(0);
    }
}

fn main() {
    let args = cli().get_matches();
    if *args.get_one::<bool>("verbose").unwrap_or(&false) {
        env::set_var("RUST_LOG", "trace");
    }

    let config = args.get_one::<String>("config_env").map(|name| {
        load_config_env(name).unwrap_or_else(|error| {
            clap::Error::raw(clap::error::ErrorKind::InvalidValue, error).exit()
        })
    });
    let output = config
        .as_ref()
        .and_then(|config| config.output.clone())
        .or_else(|| args.get_one::<String>("output_file").cloned());
    let mut jsonl = config
        .as_ref()
        .map(|config| config.writer == "jsonl")
        .unwrap_or_else(|| *args.get_one::<bool>("jsonl").unwrap_or(&false));
    let mut ben = config
        .as_ref()
        .map(|config| config.writer == "ben")
        .unwrap_or_else(|| *args.get_one::<bool>("ben").unwrap_or(&false));
    if config.is_none() {
        if output.as_deref().is_some_and(|path| path.ends_with(".ben")) {
            ben = true;
        } else if output
            .as_deref()
            .is_some_and(|path| path.ends_with(".jsonl"))
        {
            jsonl = true;
        }
    }
    let overwrite = config.is_some() || *args.get_one::<bool>("overwrite").unwrap_or(&false);

    let (reader, print) = match args.get_one::<String>("input_csv") {
        Some(file_name) => {
            let file = File::open(file_name).expect("Error opening file");
            (Box::new(BufReader::new(file)) as Box<dyn BufRead>, false)
        }
        None => (Box::new(io::stdin().lock()) as Box<dyn BufRead>, true),
    };

    let writer = match output.as_deref() {
        Some(file_name) => {
            let path = Path::new(file_name);
            if path.exists() && !overwrite {
                prompt_before_overwrite(path);
            }
            let file = File::create(path).expect("Error creating output file");
            Box::new(BufWriter::new(file)) as Box<dyn Write>
        }
        None => Box::new(io::stdout()) as Box<dyn Write>,
    };

    match (print, jsonl, ben) {
        (true, true, _) => {
            canonicalize_jsonl_from_print(reader, writer).expect("Error canonicalizing JSONL")
        }
        (true, false, true) => {
            canonicalize_ben_from_print(reader, writer).expect("Error canonicalizing BEN")
        }
        (false, true, _) => {
            canonicalize_jsonl_from_csv(reader, writer).expect("Error canonicalizing JSONL")
        }
        (false, false, true) => {
            canonicalize_ben_from_csv(reader, writer).expect("Error canonicalizing BEN")
        }
        _ => logln!("Could not determine output format"),
    }

    if let (Some(config), Some(output)) = (config, output) {
        write_metadata(Path::new(&output), &config.raw).expect("Error writing config metadata");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> String {
        json!({
            "version": 1,
            "engine": "smc",
            "io": {"graph": "/shapes/grid", "output": "/out.jsonl", "writer": "jsonl"},
            "map": {
                "pop_col": "TOTPOP",
                "n_dists": 4,
                "pop_tol": 0.01,
                "pop_bounds": []
            },
            "run": {
                "n_sims": 20,
                "rng_seed": 42,
                "compactness": 1.0,
                "resample": false,
                "adapt_k_thresh": 0.985,
                "seq_alpha": 0.5,
                "pop_temper": 0.0,
                "final_infl": 1.0,
                "est_label_mult": 1.0,
                "verbose": false,
                "silent": false,
                "tally_columns": []
            },
            "constraints": []
        })
        .to_string()
    }

    #[test]
    fn parses_valid_config() {
        let raw = valid_config();
        let config = parse_config(&raw).unwrap();
        assert_eq!(config.raw, raw);
        assert_eq!(config.output.as_deref(), Some("/out.jsonl"));
        assert_eq!(config.writer, "jsonl");
    }

    #[test]
    fn rejects_invalid_config_shapes() {
        assert!(parse_config("not json")
            .unwrap_err()
            .contains("Invalid config JSON"));
        assert!(parse_config("[]").unwrap_err().contains("JSON object"));

        let mut config: Value = serde_json::from_str(&valid_config()).unwrap();
        config["version"] = json!(1.0);
        assert!(parse_config(&config.to_string())
            .unwrap_err()
            .contains("must be an integer"));
        config["version"] = json!(2);
        assert!(parse_config(&config.to_string())
            .unwrap_err()
            .contains("Unsupported config version"));
        config["version"] = json!(1);
        config["engine"] = json!("forest");
        assert!(parse_config(&config.to_string())
            .unwrap_err()
            .contains("Expected engine 'smc'"));
        config["engine"] = json!("smc");
        config["map"].as_object_mut().unwrap().remove("n_dists");
        assert!(parse_config(&config.to_string())
            .unwrap_err()
            .contains("config.map is missing required fields: n_dists"));
    }

    #[test]
    fn config_mode_rejects_legacy_flags() {
        let error = cli()
            .try_get_matches_from(["smc_parser", "--config-env", "GERRYTOOLS_CONFIG", "--jsonl"])
            .unwrap_err();
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn uses_rustrecom_metadata_naming() {
        assert_eq!(
            metadata_path(Path::new("/tmp/foo.jsonl.ben")),
            PathBuf::from("/tmp/foo.jsonl_metadata.jsonl")
        );
    }
}
