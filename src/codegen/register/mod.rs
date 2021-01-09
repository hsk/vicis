use crate::ir::types::{TypeId, Types};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy)]
pub struct Reg(pub u16, pub u16);

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct RegUnit(pub u16, pub u16); // TODO: This is not actually register unit

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VReg(pub u32);

pub struct VRegs {
    pub map: FxHashMap<VReg, VRegData>,
    pub cur: u32,
}

pub struct VRegData {
    pub vreg: VReg,
    pub ty: TypeId,
    // ...
}

pub trait RegisterClass {
    fn for_type(types: &Types, id: TypeId) -> Self;
}

impl VRegs {
    pub fn new() -> Self {
        Self {
            map: FxHashMap::default(),
            cur: 0,
        }
    }

    pub fn add_vreg_data(&mut self, ty: TypeId) -> VReg {
        let key = VReg(self.cur);
        self.map.insert(key, VRegData { vreg: key, ty });
        self.cur += 1;
        key
    }

    pub fn type_for(&self, vreg: VReg) -> TypeId {
        self.map[&vreg].ty
    }
}
