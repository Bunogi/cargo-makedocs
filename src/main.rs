use clap::{App, Arg, SubCommand};
use serde_derive::Deserialize;
use std::fs::File;
use std::io::Read;
use std::process::{exit, Command};
use toml::value::{self, Value};

#[derive(Deserialize)]
struct CargoToml {
    dependencies: value::Table,
}

fn get_crates(toml_file: &str, excluded_crates: Vec<&str>, extra_crates: Vec<&str>) -> Vec<String> {
    let root: CargoToml = toml::from_str(toml_file).unwrap();
    root.dependencies
        .iter()
        .flat_map(|(k, v)| {
            if !excluded_crates.contains(&k.as_str()) {
                //If multiple versions of a library is flying about we need to specify the correct version
                let version = match v {
                    //If the dependency is added as [dependencies.<crate>], this needs to be handled
                    Value::Table(t) => t["version"].as_str().unwrap_or_else(|| {
                        eprintln!("{}: Missing version", k);
                        exit(1)
                    }),
                    Value::String(s) => s.as_str(),
                    _ => {
                        eprintln!("Couldn't parse Cargo.toml: invalid value in key {}", k);
                        exit(1);
                    }
                };
                vec![
                    "-p".to_string(),
                    format!("{}:{}", k, version.replace("\"", "")),
                ]
            } else {
                vec![]
            }
        })
        .chain(
            extra_crates
                .iter()
                .flat_map(|s| vec!["-p", s])
                .map(|s| s.to_string()),
        )
        .collect()
}

fn main() {
    let matches = App::new(env!("CARGO_PKG_NAME"))
        .subcommand(SubCommand::with_name("makedocs")
            .version(env!("CARGO_PKG_VERSION"))
            .about("`cargo doc` wrapper that only builds documentation for the current crate's direct dependencies. You can also explicitly include and exclude crates from being documented using the -e and -i options.")
            .author(env!("CARGO_PKG_AUTHORS"))
            .arg(
                Arg::with_name("exclude")
                    .short("e")
                    .takes_value(true)
                    .multiple(true)
                    .help("do not build documentation for a crate"),
            )
            .arg(
                Arg::with_name("include")
                    .short("i")
                    .takes_value(true)
                    .multiple(true)
                    .help("build documentation for a crate"),
            )
            .arg(
                Arg::with_name("open")
                    .short("o")
                    .long("open")
                    .help("opens the documentation for the first dependency")
            ))
        .get_matches();
    let matches = matches.subcommand_matches("makedocs").unwrap();

    let excluded_crates: Vec<&str> = match matches.values_of("exclude") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    let extra_crates: Vec<&str> = match matches.values_of("include") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    let mut file = File::open("Cargo.toml").unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    let crates = get_crates(&contents, excluded_crates, extra_crates);

    //Build command
    let mut command = Command::new("cargo");
    command.arg("doc").arg("--no-deps").args(&crates);

    //Build documentation
    command.spawn().unwrap().wait().unwrap();

    //Open docs if requested. `cargo doc` doesn't allow --open with more than one -p argument, so
    //it has to be run a second time for this, which also builds the documentation for the current
    //crate.
    if matches.is_present("open") {
        Command::new("cargo")
            .arg("doc")
            .arg("--no-deps")
            .arg("--open")
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }
}
