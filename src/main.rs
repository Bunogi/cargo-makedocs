use clap::{App, AppSettings, Arg, SubCommand};
use semver::{Version, VersionReq};
use serde_derive::Deserialize;
use std::env;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::{exit, Command};
use toml::value::{self, Value};

#[derive(Deserialize)]
struct CargoToml {
    dependencies: Option<value::Table>,
    #[serde(rename = "build-dependencies")]
    build_dependencies: Option<value::Table>,
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

#[derive(Debug)]
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
    out.sort_unstable_by(|x, y| y.version.cmp(&x.version));
    out.dedup_by(|x, y| x.name == y.name);

    //out can be zero-length if you run cargo-makedocs before cargo build.
    //Pass just the crate name to get cargo to add it
    if out.is_empty() {
        eprintln!("cargo-makedocs: Crate {} not found in Cargo.lock, please run `cargo build`. `cargo doc` might fail or doc the wrong version.", name);
        name.to_string()
    } else {
        format!("{}", out[0])
    }
    // debug_assert_eq!(out.len(), 1);
}

fn get_crates(
    toml_file: &str,
    lock_file: &str,
    excluded_crates: &[&str],
    extra_crates: &[&str],
    buildtime: bool,
) -> Result<Vec<String>, String> {
    let root: CargoToml = toml::from_str(toml_file).unwrap();
    let lock: CargoLock = toml::from_str(lock_file).unwrap();
    Ok(root
        .dependencies
        .iter()
        .flatten()
        .chain(
            //Include or ignore buildtime dependencies
            if buildtime {
                root.build_dependencies
            } else {
                None
            }
            .iter()
            .flatten(),
        )
        .filter_map(|(k, v)| {
            if !excluded_crates.contains(&k.as_str()) {
                let mut changed_name = None;
                //If multiple versions of a library is flying about we need to specify the correct version
                let version = match v {
                    //If the dependency is added as [dependencies.<crate>], this needs to be handled
                    Value::Table(t) => {
                        if let Some(name) = t.get("package") {
                            //Package is renamed
                            changed_name = Some(name.as_str().unwrap());
                        }
                        if let Some(v) = t.get("version") {
                            v.as_str().unwrap()
                        } else if t.get("path").is_some() || t.get("git").is_some() {
                            "*" //Assume that the user is developing the dependency if using a path
                                //and that if using git, wants the latest version available
                        } else {
                            eprintln!("cargo-makedocs: dependency {} is invalid", k);
                            exit(1);
                        }
                    }
                    Value::String(s) => s,
                    _ => {
                        eprintln!(
                            "cargo-makedocs: couldn't parse Cargo.toml: invalid value in key {}",
                            k
                        );
                        exit(1);
                    }
                };

                //Get the compatible version from Cargo.lock to always build the correct version
                Some(correct_version(&lock, changed_name.unwrap_or(k), &version))
            } else {
                None
            }
        })
        .chain(extra_crates.iter().map(std::string::ToString::to_string))
        .collect())
}

fn create_arguments(input: &Vec<String>) -> Vec<&str> {
    input.iter().flat_map(|s| vec!["-p", s]).collect()
}

//Looks for Cargo.toml in every directory above the current directory.
fn find_rootdir() -> Result<PathBuf, String> {
    match env::current_dir() {
        Ok(dir) => match dir
            .ancestors()
            .flat_map(|a| {
                a.read_dir()
                    .map(|f| f.map(|entry| entry.unwrap().path()))
                    .unwrap()
            })
            .find(|buf| buf.file_name() == Some("Cargo.toml".as_ref()))
        {
            Some(mut s) => {
                s.pop();
                Ok(s)
            }
            None => Err("Cannot find Cargo.toml in any ancestor directory".to_string()),
        },
        Err(e) => Err(format!("Can't find Cargo.toml: {}", e)),
    }
}

fn run(matches: &clap::ArgMatches) -> Result<(), String> {
    let excluded_crates: Vec<&str> = match matches.values_of("exclude") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    let extra_crates: Vec<&str> = match matches.values_of("include") {
        Some(ex) => ex.collect(),
        None => vec![],
    };

    //Cargo root directory
    let dir = match find_rootdir() {
        Ok(path) => path.canonicalize().unwrap(),
        Err(e) => return Err(e),
    };

    let mut cargo_toml = String::new();
    File::open(dir.join("Cargo.toml"))
        .map_err(|e| format!("Couldn't open Cargo.toml: {}", e))?
        .read_to_string(&mut cargo_toml)
        .unwrap();

    let mut lock_file = String::new();
    File::open(dir.join("Cargo.lock"))
        .map_err(|e| format!("Couldn't open Cargo.lock: {}", e))?
        .read_to_string(&mut lock_file)
        .unwrap();

    let crates = get_crates(
        &cargo_toml,
        &lock_file,
        &excluded_crates,
        &extra_crates,
        !matches.is_present("no-buildtime"),
    )?;

    //Build command
    let mut command = Command::new("cargo");
    command
        .arg("doc")
        .arg("--no-deps")
        .args(&create_arguments(&crates));

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

    if crates.is_empty() {
        return Err("Found no crates to document".into());
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
    Ok(())
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
                    .requires("root")
            ).arg(
                Arg::with_name("no-buildtime")
                  .short("n")
                  .long("no-buildtime")
                  .help("Ignore buildtime dependencies")
            )
        )
        .get_matches();

    let matches = matches.subcommand_matches("makedocs").unwrap(); //Cannot panic when run through cargo

    match run(matches) {
        Ok(()) => (),
        Err(e) => {
            eprintln!("cargo-makedocs: {}", e);
            exit(1)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_crates_buildtime_deps() {
        use super::get_crates;
        let cargo_toml = r#"dependencies = {renamed = {package = "foo", version = "1.3"}}"#;
        let cargo_lock = r#"[[package]]
name="foo"
version="1.3.5""#;
        let crates = get_crates(cargo_toml, cargo_lock, &[], &[], true).unwrap();
        assert_eq!(crates, ["foo:1.3.5"]);
    }
    #[test]
    fn get_crates_include_exclude_crate() {
        use super::get_crates;
        let cargo_toml = r#"dependencies = {some-crate = "1.0.0", foo = "1.2.0"}"#;
        let cargo_lock = r#"[[package]]
name = "some-crate"
version="1.3.2"
[[package]]
name="foo"
version="1.3.5"
[[package]]
name = "include-me"
version="1.2.3""#;
        let crates = get_crates(
            cargo_toml,
            &cargo_lock,
            &["some-crate"],
            &["include-me"],
            true,
        )
        .unwrap();
        assert_eq!(crates, ["foo:1.3.5", "include-me"]);
    }

    #[test]
    fn get_crates_from_path() {
        use super::get_crates;
        let cargo_toml = r#"dependencies = {some-crate = { path = "some-crate" }}"#;
        let cargo_lock = r#"[[package]]
name = "some-crate"
version="1.3.2"
[[package]]
name = "some-crate"
version = "1.3.6""#;
        let crates = get_crates(cargo_toml, cargo_lock, &[], &[], true).unwrap();
        assert_eq!(crates, ["some-crate:1.3.6"]);
    }

    #[test]
    fn get_version_from_git() {
        use super::get_crates;
        let cargo_toml = r#"dependencies = {libc = { git = "https://github.com/rust-lang/libc" }}"#;
        let cargo_lock = r#"[[package]]
name = "libc"
version = "0.2.43"
source = "git+https://github.com/rust-lang/libc#9c5e70ae306463a23ec02179ac2c9fe05c3fb44e"
"#;
        let crates = get_crates(cargo_toml, cargo_lock, &[], &[], true).unwrap();
        assert_eq!(crates, ["libc:0.2.43"]);
    }
}
