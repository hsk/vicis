extern crate structopt;
extern crate vicis_core;
extern crate vicis_interpreter;

use std::{fs, process};
use structopt::StructOpt;
use vicis_core::ir::module;
use vicis_interpreter::interpreter;

#[derive(Debug, StructOpt)]
#[structopt(name = "i")]
pub struct Opt {
    pub ir_file: String,

    #[structopt(long = "load")]
    pub libs: Vec<String>,
}

fn main() {
    let opt = Opt::from_args();
    let ir = fs::read_to_string(opt.ir_file).expect("failed to load *.ll file");
    let module = module::parse_assembly(ir.as_str()).expect("failed to parse LLVM Assembly");
    let main = module
        .find_function_by_name("main")
        .expect("failed to lookup 'main'");
    let ctx = interpreter::Context::new(&module)
        .with_libs(opt.libs)
        .expect("failed to load library");
    let ret = interpreter::run_function(&ctx, main, vec![]);
    process::exit(ret.expect("unknown error").sext_to_i64().unwrap_or(0) as i32)
}
