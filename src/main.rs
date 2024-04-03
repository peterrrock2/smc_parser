use ben::encode::BenEncoder;
use clap::{Arg, ArgAction, Command};
use serde_json::json;
use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader, BufWriter, Write},
};

macro_rules! log {
    ($($arg:tt)*) => {{
        if let Ok(log_level) = std::env::var("RUST_LOG") {
            if log_level.to_lowercase() == "trace" {
                eprint!($($arg)*);
            }
        }
    }}
}

macro_rules! logln {
    ($($arg:tt)*) => {{
        if let Ok(log_level) = std::env::var("RUST_LOG") {
            if log_level.to_lowercase() == "trace" {
                eprintln!($($arg)*);
            }
        }
    }}
}

fn canonicalize_jsonl_from_print<R: BufRead, W: Write>(reader: R, mut writer: W) -> io::Result<()> {
    let mut start = false;

    let mut sample = 1;

    for line in reader.lines() {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if !start && str_line == "Now printing the plans:".to_string() {
            start = true;
            continue;
        }

        if !start {
            continue;
        }

        log!("Processing sample {}\r", sample);
        let json_line = json!({
            "assignment": serde_json::from_str::<Vec<u16>>(&str_line)
                .expect(
                    &format!("Failed to parse line: {} with value {} as Vec<u16>",
                        sample,
                        str_line)
                ),
            "sample": sample
        })
        .to_string();

        writeln!(writer, "{}", json_line)?;
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
    let mut sample = 1;

    let mut csv_reader = csv::Reader::from_reader(reader);

    for line in csv_reader.records() {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        log!("Processing sample {}\r", sample);
        let assignment = str_line
            .iter()
            .map(|x| {
                x.parse::<u16>()
                    .expect(format!("Failed to parse {} as u16 in sample {}", x, sample).as_str())
            })
            .collect::<Vec<u16>>();

        let json_line = json!({
            "assignment": assignment[1..],
            "sample": sample
        })
        .to_string();

        writeln!(writer, "{}", json_line)?;
        sample += 1;
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

        if !start && str_line == "Now printing the plans:".to_string() {
            start = true;
            continue;
        }

        if !start {
            continue;
        }

        log!("Processing sample {}\r", sample);
        ben_encoder
            .write_assignment(serde_json::from_str::<Vec<u16>>(&str_line).expect(&format!(
                "Failed to parse line: {} with value {} as Vec<u16>",
                sample, str_line
            )))
            .expect("Failed to encode as Ben");

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
    let mut sample = 1;

    let mut csv_reader = csv::Reader::from_reader(reader);

    let mut ben_encoder = BenEncoder::new(writer);

    for line in csv_reader.records() {
        let str_line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        log!("Processing sample {}\r", sample);
        let assignment = str_line
            .iter()
            .map(|x| {
                x.parse::<u16>()
                    .expect(format!("Failed to parse {} as u16 in sample {}", x, sample).as_str())
            })
            .collect::<Vec<u16>>();

        ben_encoder
            .write_assignment(assignment[1..].to_vec())
            .expect("Failed to encode as Ben");
        sample += 1;
    }
    logln!();
    logln!("Done!");
    Ok(())
}

fn main() {
    let args = Command::new("canonicalize_jsonl")
        .version("0.1.0")
        .about("Canonicalize jsonl file")
        .arg(
            Arg::new("input_csv")
                .short('i')
                .long("input-csv")
                .help("Path to the input jsonl file")
                .required(false),
        )
        .arg(
            Arg::new("output_file")
                .short('o')
                .long("output-file")
                .help("Path to the output jsonl file")
                .required(false),
        )
        .arg(
            Arg::new("jsonl")
                .short('j')
                .long("jsonl")
                .help("Convert from an assignment csv output to jsonl")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("ben")
                .short('b')
                .long("ben")
                .help("Convert from an assignment csv output to ben")
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
        .get_matches();

    if *args.get_one::<bool>("verbose").unwrap_or(&false) {
        env::set_var("RUST_LOG", "trace");
    }

    let mut jsonl = *args.get_one::<bool>("jsonl").unwrap_or(&false);
    let mut ben = *args.get_one::<bool>("ben").unwrap_or(&false);
    let mut print = true;

    let reader = match args.get_one("input_csv").map(String::as_str) {
        Some(file_name) => {
            print = false;
            let file = File::open(file_name).expect("Error opening file");
            Box::new(BufReader::new(file)) as Box<dyn BufRead>
        }
        None => Box::new(io::stdin().lock()) as Box<dyn BufRead>,
    };

    let writer = match args.get_one("output_file").map(String::as_str) {
        Some(file_name) => {
            if file_name.ends_with(".ben") {
                ben = true;
            } else if file_name.ends_with(".jsonl") {
                jsonl = true;
            }

            let path = std::path::Path::new(file_name);
            if path.exists() && !*args.get_one::<bool>("overwrite").unwrap_or(&false) {
                eprint!(
                    "File {:?} already exists. Would you like to overwrite? y/[n]: ",
                    path
                );
                let mut response = String::new();
                io::stdin()
                    .read_line(&mut response)
                    .expect("Error reading response");

                if response.trim() != "y" {
                    std::process::exit(0);
                }
            }
            let file = File::create(file_name).expect("Error creating file");
            Box::new(BufWriter::new(file)) as Box<dyn Write>
        }
        None => Box::new(io::stdout()) as Box<dyn Write>,
    };

    if print {
        if jsonl {
            canonicalize_jsonl_from_print(reader, writer).expect("Error canonicalizing jsonl");
        } else if ben {
            canonicalize_ben_from_print(reader, writer).expect("Error canonicalizing ben");
        } else {
            logln!("Could not determine output format");
        }
    } else {
        if jsonl {
            canonicalize_jsonl_from_csv(reader, writer).expect("Error canonicalizing jsonl");
        } else if ben {
            canonicalize_ben_from_csv(reader, writer).expect("Error canonicalizing ben");
        } else {
            logln!("Could not determine output format");
        }
    }
}
