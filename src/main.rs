extern crate clap;
extern crate semver;
extern crate serde_derive;
extern crate toml;

use clap::{App, AppSettings, Arg, SubCommand};
use semver::{Version, VersionReq};
use serde_derive::Deserialize;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::process::{exit, Command};
use toml::value::{self, Value};

#[derive(Deserialize)]
struct CargoToml {
    dependencies: value::Table,
}

#[derive(Deserialize)]
struct CargoLock {
    package: Vec<LockEntry>,
}

#[derive(Deserialize)]
struct LockEntry {
    name: String,
    version: String,
}

struct Crate<'a> {
    pub name: &'a str,
    pub version: Version,
}

impl<'a> fmt::Display for Crate<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.name, self.version)
    }
}

//Assumes the syntax of cargo.lock is correct
fn correct_version<'a>(lock: &'a CargoLock, name: &str, version: &str) -> String {
    let mut out = Vec::new();
    let crate_version = VersionReq::parse(version).unwrap();
    lock.package
        .iter()
        .filter(|x| x.name == name)
        .for_each(|p| {
            //Push the matching version numbers onto out
            let lock_version = Version::parse(&p.version.as_str()).unwrap();
            if crate_version.matches(&lock_version) {
                out.push(Crate {
                    name: &p.name,
                    version: lock_version,
                });
            }
        });

    //Ensure we use the most up to date, compatible crate
    out.sort_unstable_by(|x, y| x.version.cmp(&y.version));
    out.dedup_by(|x, y| x.name == y.name);
    debug_assert_eq!(out.len(), 1);

    format!("{}", out[0])
}

fn get_crates(
    toml_file: &str,
    lock_file: &str,
    excluded_crates: &[&str],
    extra_crates: &[&str],
) -> Vec<String> {
    let root: CargoToml = toml::from_str(toml_file).unwrap();
    let lock: CargoLock = toml::from_str(lock_file).unwrap();
    root.dependencies
        .iter()
        .flat_map(|(k, v)| {
            if !excluded_crates.contains(&k.as_str()) {
                //If multiple versions of a library is flying about we need to specify the correct version
                let version = match v {
                    //If the dependency is added as [dependencies.<crate>], this needs to be handled
                    Value::Table(t) => {
                        if let Some(v) = t.get("version") {
                            v.as_str().unwrap()
                        } else if t.get("path").is_some() {
                            "*" //Assume that the user is developing the dependency if using a path
                        } else {
                            eprintln!("Error: dependency {} is invalid", k);
                            exit(1);
                        }
                    }
                    Value::String(s) => s,
                    _ => {
                        eprintln!("Couldn't parse Cargo.toml: invalid value in key {}", k);
                        exit(1);
                    }
                };

                //Get the compatible version from Cargo.lock to always build the correct version
                let name = correct_version(&lock, k, &version);
                vec!["-p".to_string(), name]
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
        .setting(AppSettings::SubcommandRequired)
        .subcommand(SubCommand::with_name("makedocs")
            .version(env!("CARGO_PKG_VERSION"))
            .about("`cargo doc` wrapper that only builds documentation for the current crate's direct dependencies, by scanning Cargo.toml and Cargo.lock. You can also explicitly include and exclude crates from being documented using the -e and -i options.")
            .author(env!("CARGO_PKG_AUTHORS"))
            .arg(
                Arg::with_name("exclude")
                    .short("e")
                    .takes_value(true)
                    .multiple(true)
                    .help("do not build documentation for a crate"),
            ).arg(
                Arg::with_name("include")
                    .short("i")
                    .takes_value(true)
                    .multiple(true)
                    .help("build documentation for a crate"),
            ).arg(
                Arg::with_name("open")
                    .short("o")
                    .long("open")
                    .help("opens the built documentation")
            ).arg(
                Arg::with_name("root")
                    .short("r")
                    .long("root")
                    .help("Build the documentation for the root crate")
            ).arg(
                Arg::with_name("document-private-items")
                    .short("d")
                    .long("document-private-items")
                    .help("passes --document-private-items when building the docs for the root crate")
                    .requires("root")))
        .get_matches();

    let matches = matches.subcommand_matches("makedocs").unwrap(); //Cannot panic

    let excluded_crates: Vec<&str> = match matches.values_of("exclude") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    let extra_crates: Vec<&str> = match matches.values_of("include") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    let mut cargo_toml = String::new();
    File::open("Cargo.toml")
        .unwrap()
        .read_to_string(&mut cargo_toml)
        .unwrap();

    let mut lock_file = String::new();
    File::open("Cargo.lock")
        .unwrap()
        .read_to_string(&mut lock_file)
        .unwrap();

    let crates = get_crates(&cargo_toml, &lock_file, &excluded_crates, &extra_crates);

    //Build command
    let mut command = Command::new("cargo");
    command.arg("doc").arg("--no-deps").args(&crates);

    if matches.is_present("document-private-items") {
        command.arg("--document-private-items");
    }

    if matches.is_present("root") {
        let mut pkg_id_command = Command::new("cargo");
        pkg_id_command.arg("pkgid");
        let pkg_id =
            String::from_utf8_lossy(&pkg_id_command.output().unwrap().stdout).replace("\n", "");
        command.arg("-p").arg(pkg_id);
    }

    //Build documentation
    command.spawn().unwrap().wait().unwrap();

    //Open docs if requested. `cargo doc` doesn't allow --open with more than one -p argument, so
    //it has to be run a second time for this.
    if matches.is_present("open") {
        let mut command = Command::new("cargo");
        command.arg("doc").arg("--no-deps").arg("--open");

        if !matches.is_present("root") {
            command.arg("-p").arg(&crates[1]);
        }

        command.spawn().unwrap().wait().unwrap();
    }
}
