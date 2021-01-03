pub mod inst_selection;
pub mod instruction;
pub mod register;

use super::Target;
use crate::codegen::{
    instruction::Instruction as MachInstruction, target::x86_64::instruction::InstructionData,
};
use crate::ir::{function::Data, instruction::Instruction};

pub struct X86_64 {}

impl Target for X86_64 {
    type InstData = instruction::InstructionData;

    fn select_patterns(
    ) -> Vec<fn(&Data, &Instruction) -> Option<Vec<MachInstruction<InstructionData>>>> {
        vec![inst_selection::ret]
    }
}
