#[macro_use]
extern crate log;
extern crate simple_logger;
extern crate getopts;
extern crate glob;
extern crate regex;
extern crate tempfile;
extern crate ansi_term;

use getopts::Options;
use std::collections::HashMap;
use std::env;
use std::path::PathBuf;
use std::str;
mod drt;
use drt::diff::diff;
use drt::diff::DiffStatus;
use drt::Mode;
use drt::SrcFile;
use drt::DestFile;
use drt::GenFile;
use drt::template::{create_from, generate_recommended_file, replace_line};
use std::io::Error;
use std::slice::Iter;
use log::LevelFilter;
use drt::userinput::ask;
use std::process::Command;
use std::io::{self, Write};
use ansi_term::Colour::{Green, Yellow};
use simple_logger::SimpleLogger;

fn create_or_diff(
    mode: Mode, 
    template: & SrcFile,
    dest: & DestFile,
    gen: & GenFile
) -> Result<DiffStatus, Error> {
    if dest.exists() {
            debug!("create_or_diff: diff");
            diff(gen.path(), dest.path());
            create_from(mode, template, gen, dest)
    } else {
        create_from(mode, template, gen, dest)
    }
}
fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [options]", program);
    print!("{}", opts.usage(&brief));
}
#[derive(Debug)]
enum Action {
    Template(String,String),
    Execute(String),
    None,
}
#[derive(Debug)]
enum Type {
    Template,
    Execute,
    //InputFile,
    //OutputFile,
    Variable,
    Unknown
}
#[test]
fn test_parse_type() {
    match parse_type(&String::from("t")) { Type::Template => {}, _ => panic!("expected Template"), }
    match parse_type(&String::from("x")) { Type::Execute => {}, _ => panic!("expected Execute"), }
    match parse_type(&String::from("v")) { Type::Variable => {}, _ => panic!("expected Template"), }
}

fn parse_type(input: &str) -> Type {
    match input {
        "t" => Type::Template,
        "x" => Type::Execute,
        "v" => Type::Variable,
        _ => { debug!("Unknown {}", input); Type::Unknown},
    }
}
fn process_template_file<'t>(
    mode: Mode,
    vars: &'t HashMap<&'_ str, &'_ str>,
    template: & SrcFile,
    dest: & DestFile
) -> Result<DiffStatus, Error> {
    let gen = generate_recommended_file(vars, template)?;
    create_or_diff(mode, template, dest, &gen)
}
#[test]
fn test_execute_active() {
    execute_active("/bin/true");
    execute_active("/bin/false");
    execute_active("echo echo_ping");
}
fn execute_active(cmd: &str) {
	let parts: Vec<&str> = cmd.split(' ').collect();
	let output = Command::new(parts[0])
		.args(&parts[1..])
		.output()
		.expect("cmd failed");
    println!("{} {}", Green.paint("LIVE: run "), Green.paint(cmd) );
	io::stdout().write_all(&output.stdout).expect("error writing to stdout");
	println!("status code: {}", output.status.code().unwrap());
}
fn execute_interactive(cmd: &str) {
	match ask(&format!("run (y/n): {}", cmd)) {
		'n' => println!("{} {}", Yellow.paint("WOULD: run "), Yellow.paint(cmd) ),
		'y' => execute_active(cmd),
		_ => execute_interactive(cmd)
	}
}
fn execute(
    mode: Mode,
    cmd: &str
) -> Result<(), Error> {
    match mode {
        Mode::Interactive => { execute_interactive(cmd); },
        Mode::Passive => println!("{} {}", Yellow.paint("WOULD: run "), Yellow.paint(cmd) ),
        Mode::Active=> { execute_active(cmd) }
    }
    Ok(())
}

fn do_action<'g>(
    mode: Mode,
    vars: &'g HashMap<&'g str, &'g str>,
    action: Action,
) -> Result<(), Error> {
    match action {
        Action::Template(template_file_name, output_file_name) => {
            let template_file = SrcFile::new(PathBuf::from(template_file_name));
            let output_file = DestFile::new(PathBuf::from(output_file_name));

            process_template_file(mode, &vars, &template_file, &output_file)?;
            Ok(())
        },
        Action::Execute(cmd) => {
            match replace_line(vars, cmd.clone())? {
                Some(new_cmd) => execute(mode, &new_cmd)?,
                None => execute(mode, &cmd)?
            }
            Ok(())
        },
        Action::None => {
            Ok(())
        }
    }
}

#[test]
fn test_do_action() {
    let mut vars: HashMap<&str, &str> = HashMap::new();
    vars.insert("value", "unit_test");
    let template = Action::Template( String::from("template/test.config"), String::from("template/out.config"));
    match do_action(Mode::Passive, &vars, template) {
        Ok(_) =>  {}
        Err(e) => panic!("{}", e)
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("D", "debug", "debug logging");
    opts.optflag("i", "interactive", "ask before overwrite");
    opts.optflag("a", "active", "overwrite without asking");
    opts.optflag("h", "help", "print this help menu");
    let matches = opts.parse(&args[1..]).unwrap();

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return;
    }
    if matches.opt_present("debug") {
        SimpleLogger::new().with_level(LevelFilter::Trace).init().expect("log inti error");
    } else {
        SimpleLogger::new().with_level(LevelFilter::Warn).init().expect("log inti error");
    }
    let drt_active_env = env::var("DRT_ACTIVE").is_ok();
    if drt_active_env {
        debug!("DRT_ACTIVE enabled DRT_ACTIVE= {:#?}", env::var("DRT_ACTIVE").unwrap());
    } else {
        debug!("DRT_ACTIVE not set");
    }
    let mode = if matches.opt_present("interactive") {
        Mode::Interactive
    } else if matches.opt_present("active") | drt_active_env {
        Mode::Active
    } else {
        Mode::Passive
    };
    let mut vars: HashMap<&str, &str> = HashMap::new();
    {
        let mut input_list:Iter<String>= matches.free.iter(); 
        while let Some(input) =  input_list.next() {
            let t:Type = parse_type(input);
			let mut cmd = String::new();
            let action = match t {
                Type::Template => {
                    let infile = String::from(input_list.next().expect("expected template: tp template output"));
                    let outfile = String::from(input_list.next().expect("expected output: tp template output"));
                    Action::Template(infile, outfile)
                },
                Type::Variable=> {
                    let key = input_list.next().expect("expected key: v key value");
                    let val = input_list.next().expect("expected value: v key value");
                    vars.insert(key,val);
                    Action::None
                },
                Type::Execute => {
        			while let Some(input) = input_list.next() {
						if cmd.is_empty() {
							cmd.push_str(&input.to_string());
						} else {
							cmd.push_str(" ");
							cmd.push_str(&input.to_string());
						}
					}
					//let cmd_str: &str = cmd.as_str();
                    Action::Execute(cmd)
                },
                Type::Unknown => {
                    panic!("Unknown type: {}", input);
                }
            };
            //debug!("vars {:#?}", &vars);
            debug!("action {:#?}", action);
            match do_action(mode, &vars, action) {
                Ok(a) => a,
                Err(e) => { 
                    println!("{}", e);
                }
            }
        }
    }
}
