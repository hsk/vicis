use super::{
    call_conv::CallConvKind,
    function::{
        basic_block::BasicBlockId as MachBasicBlockId,
        data::Data,
        instruction::{Instruction as MachInstruction, InstructionInfo as II},
        layout::Layout,
        slot::{SlotId, Slots},
        Function as MachFunction,
    },
    isa::TargetIsa,
    module::Module as MachModule,
    register::VReg,
};
use anyhow::Result;
use id_arena::Arena;
use rustc_hash::{FxHashMap, FxHashSet};
use std::{error::Error, fmt, mem};
use vicis_core::ir::{
    function::{
        basic_block::BasicBlockId as IrBasicBlockId,
        data::Data as IrData,
        instruction::{Instruction as IrInstruction, InstructionId as IrInstructionId, Opcode},
        Function as IrFunction, Parameter,
    },
    module::Module as IrModule,
    types::Types,
};

pub trait Lower<T: TargetIsa> {
    fn lower(ctx: &mut LoweringContext<T>, inst: &IrInstruction) -> Result<()>;
    fn copy_args_to_vregs(ctx: &mut LoweringContext<T>, params: &[Parameter]) -> Result<()>;
}

// TODO: So confusing. Need refactoring.
pub struct LoweringContext<'a, T: TargetIsa> {
    pub ir_data: &'a IrData,
    pub mach_data: &'a mut Data<<T::InstInfo as II>::Data>,
    pub slots: &'a mut Slots<T>,
    pub inst_id_to_slot_id: &'a mut FxHashMap<IrInstructionId, SlotId>,
    pub arg_idx_to_vreg: &'a mut FxHashMap<usize, VReg>,
    pub inst_seq: &'a mut Vec<MachInstruction<<T::InstInfo as II>::Data>>,
    pub types: &'a Types,
    pub inst_id_to_vreg: &'a mut FxHashMap<IrInstructionId, VReg>,
    pub merged_inst: &'a mut FxHashSet<IrInstructionId>,
    pub block_map: &'a FxHashMap<IrBasicBlockId, MachBasicBlockId>,
    pub call_conv: CallConvKind,
    pub cur_block: IrBasicBlockId,
}

#[derive(Debug)]
pub enum LoweringError {
    Todo,
}

pub fn compile_module<T: TargetIsa>(isa: T, module: &IrModule) -> Result<MachModule<T>> {
    let mut functions = Arena::new();

    for (_, function) in module.functions() {
        functions.alloc(compile_function(isa, function)?);
    }

    let mut mach_module = MachModule {
        name: module.name().to_owned(),
        source_filename: module.source_filename().to_owned(),
        target: module.target().to_owned(),
        functions,
        attributes: module.attributes().to_owned(),
        global_variables: module.global_variables().to_owned(),
        types: module.types.clone(),
        isa,
    };

    for pass in T::module_pass_list() {
        pass(&mut mach_module)?
    }

    Ok(mach_module)
}

pub fn compile_function<T: TargetIsa>(isa: T, function: &IrFunction) -> Result<MachFunction<T>> {
    let mut slots = Slots::new(isa);
    let mut data = Data::new();
    let mut layout = Layout::new();
    let mut block_map = FxHashMap::default();

    // Create machine basic blocks
    for block_id in function.layout.block_iter() {
        let new_block_id = data.create_block();
        layout.append_block(new_block_id);
        block_map.insert(block_id, new_block_id);
    }

    // Insert preds and succs
    for block_id in function.layout.block_iter() {
        let new_block_id = block_map[&block_id];
        let block = &function.data.basic_blocks[block_id];
        let new_block = data.basic_blocks.get_mut(new_block_id).unwrap();
        for pred in &block.preds {
            new_block.preds.insert(block_map[pred]);
        }
        for succ in &block.succs {
            new_block.succs.insert(block_map[succ]);
        }
    }

    let mut inst_id_to_slot_id = FxHashMap::default();
    let mut inst_id_to_vreg = FxHashMap::default();
    let mut arg_idx_to_vreg = FxHashMap::default();
    let mut merged_inst = FxHashSet::default();
    let call_conv = T::default_call_conv();

    for (i, block_id) in function.layout.block_iter().enumerate() {
        let mut insts_seq = vec![];
        let mut inst_seq = vec![];
        let mut prologue_seq = vec![];

        // entry block
        if i == 0 {
            T::Lower::copy_args_to_vregs(
                &mut LoweringContext {
                    ir_data: &function.data,
                    mach_data: &mut data,
                    slots: &mut slots,
                    inst_id_to_slot_id: &mut inst_id_to_slot_id,
                    inst_seq: &mut prologue_seq,
                    arg_idx_to_vreg: &mut arg_idx_to_vreg,
                    types: &function.types,
                    inst_id_to_vreg: &mut inst_id_to_vreg,
                    merged_inst: &mut merged_inst,
                    block_map: &block_map,
                    call_conv,
                    cur_block: block_id,
                },
                function.params(),
            )?;
        }

        // Only handle Alloca and Phi insts
        // TODO: Refactoring
        for inst_id in function.layout.inst_iter(block_id) {
            let inst = function.data.inst_ref(inst_id);
            if inst.opcode != Opcode::Alloca && inst.opcode != Opcode::Phi {
                break;
            }
            T::Lower::lower(
                &mut LoweringContext {
                    ir_data: &function.data,
                    mach_data: &mut data,
                    slots: &mut slots,
                    inst_id_to_slot_id: &mut inst_id_to_slot_id,
                    inst_seq: &mut prologue_seq,
                    arg_idx_to_vreg: &mut arg_idx_to_vreg,
                    types: &function.types,
                    inst_id_to_vreg: &mut inst_id_to_vreg,
                    merged_inst: &mut merged_inst,
                    block_map: &block_map,
                    call_conv,
                    cur_block: block_id,
                },
                inst,
            )?;
        }

        for inst_id in function.layout.inst_iter(block_id).rev() {
            let inst = function.data.inst_ref(inst_id);

            if inst.opcode == Opcode::Alloca || inst.opcode == Opcode::Phi {
                break;
            }

            // Check if `inst` has no side effects and has user instructions placed in
            // the same basic block
            if !inst.opcode.has_side_effects()
                && function
                    .data
                    .users_of(inst_id)
                    .iter()
                    .all(|&user| function.data.inst_ref(user).parent == inst.parent)
            {
                continue;
            }

            T::Lower::lower(
                &mut LoweringContext {
                    ir_data: &function.data,
                    mach_data: &mut data,
                    slots: &mut slots,
                    inst_id_to_slot_id: &mut inst_id_to_slot_id,
                    inst_seq: &mut inst_seq,
                    arg_idx_to_vreg: &mut arg_idx_to_vreg,
                    types: &function.types,
                    inst_id_to_vreg: &mut inst_id_to_vreg,
                    merged_inst: &mut merged_inst,
                    block_map: &block_map,
                    call_conv,
                    cur_block: block_id,
                },
                inst,
            )?;

            insts_seq.push(mem::take(&mut inst_seq));
        }

        insts_seq.push(prologue_seq);

        for inst_seq in insts_seq.into_iter().rev() {
            for mach_inst in inst_seq {
                let mach_inst = data.create_inst(mach_inst);
                layout.append_inst(mach_inst, block_map[&block_id])
            }
        }
    }

    Ok(MachFunction {
        name: function.name.to_owned(),
        is_var_arg: function.is_var_arg,
        result_ty: function.result_ty,
        params: function.params.clone(),
        preemption_specifier: function.preemption_specifier,
        attributes: function.func_attrs.clone(),
        data,
        layout,
        slots,
        types: function.types.clone(),
        is_prototype: function.is_prototype(),
        isa,
        call_conv,
    })
}

impl<'a, T: TargetIsa> LoweringContext<'a, T> {
    pub fn set_output_for_inst(&mut self, id: IrInstructionId, vreg: VReg) {
        self.inst_id_to_vreg.insert(id, vreg);
    }

    pub fn mark_as_merged(&mut self, inst: IrInstructionId) {
        self.merged_inst.insert(inst);
    }

    pub fn is_merged(&self, inst: IrInstructionId) -> bool {
        self.merged_inst.contains(&inst)
    }
}

impl Error for LoweringError {}

impl fmt::Display for LoweringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Todo => write!(f, "Todo"),
        }
    }
}
