pub mod x86_64;

use crate::codegen::{
    call_conv::CallConvKind,
    function::instruction::InstructionInfo,
    lower,
    module::Module,
    register::{RegisterClass, RegisterInfo},
};
use anyhow::Result;
use vicis_core::ir::types::{Type, Types};

pub trait TargetIsa: Copy {
    type InstInfo: InstructionInfo;
    type RegClass: RegisterClass;
    type RegInfo: RegisterInfo;
    type Lower: lower::Lower<Self>;

    fn module_pass_list() -> Vec<fn(&mut Module<Self>) -> Result<()>>;
    fn default_call_conv() -> CallConvKind;
    fn type_size(types: &Types, ty: Type) -> u32;
}
