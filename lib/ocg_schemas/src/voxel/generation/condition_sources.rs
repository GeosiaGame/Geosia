//! Default Condition Source types.

use serde::{Serialize, Deserialize};
use bevy_math::IVec3;

use crate::voxel::{generation::{ConditionSource, Context}, biome::ConditionSrc};

/// Always-true condition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlwaysTrueCondition();

impl AlwaysTrueCondition {
    /// Boxes a new `AlwaysTrueCondition` automatically
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self())
    }
}

#[typetag::serde]
impl ConditionSource for AlwaysTrueCondition {
    #[inline(never)]
    fn test(&self, _pos: &IVec3, _context: &Context) -> bool {
        true
    }
}

/// Minimum Y level condition. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct YLevelCondition {
    min_y: i32
}
impl YLevelCondition {
    /// Helper for creating new `YLevelCondition`s
    pub fn new(min_y: i32) -> Self {
        Self {
            min_y: min_y,
        }
    }

    /// Boxes a new `YLevelCondition` automatically
    pub fn new_boxed(min_y: i32) -> Box<Self> {
        Box::new(Self::new(min_y))
    }
}

#[typetag::serde]
impl ConditionSource for YLevelCondition {
    #[inline(never)]
    fn test(&self, pos: &IVec3, _context: &Context) -> bool {
        pos.y >= self.min_y
    }
}

/// Invert condition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotCondition {
    condition: Box<ConditionSrc>
}
impl NotCondition {
    /// Helper for creating new `YLevelCondition`s
    pub fn new(condition: Box<ConditionSrc>) -> Self {
        Self {
            condition: condition,
        }
    }

    /// Boxes a new `NotCondition` automatically
    pub fn new_boxed(condition: Box<ConditionSrc>) -> Box<Self> {
        Box::new(Self::new(condition))
    }
}

#[typetag::serde]
impl ConditionSource for NotCondition {
    #[inline(never)]
    fn test(&self, pos: &IVec3, context: &Context) -> bool {
        !self.condition.test(pos, context)
    }
}

/// Minimum ground Y level condition. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroundLevelCondition();

impl GroundLevelCondition {
    /// Boxes a new `GroundLevelCondition` automatically
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self())
    }
}

#[typetag::serde]
impl ConditionSource for GroundLevelCondition {
    #[inline(never)]
    fn test(&self, pos: &IVec3, context: &Context) -> bool {
        context.ground_y == pos.y
    }
}

/// Under ground Y level condition. Exclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnderGroundLevelCondition();

impl UnderGroundLevelCondition {
    /// Boxes a new `UnderGroundLevelCondition` automatically
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self())
    }
}

#[typetag::serde]
impl ConditionSource for UnderGroundLevelCondition {
    #[inline(never)]
    fn test(&self, pos: &IVec3, context: &Context) -> bool {
        context.ground_y > pos.y
    }
}

/// Under ground Y level condition. Exclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnderSeaLevelCondition();

impl UnderSeaLevelCondition {
    /// Boxes a new `UnderSeaLevelCondition` automatically
    pub fn new_boxed() -> Box<Self> {
        Box::new(Self())
    }
}

#[typetag::serde]
impl ConditionSource for UnderSeaLevelCondition {
    #[inline(never)]
    fn test(&self, pos: &IVec3, context: &Context) -> bool {
        context.sea_level > pos.y
    }
}


/// Ground Y level condition with an offset. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OffsetGroundLevelCondition {
    offset: i32,
}
impl OffsetGroundLevelCondition {
    /// Helper for creating new `OffsetGroundLevelCondition`s
    pub fn new(offset: i32) -> Self {
        Self {
            offset: offset
        }
    }

    /// Boxes a new `OffsetGroundLevelCondition` automatically
    pub fn new_boxed(offset: i32) -> Box<Self> {
        Box::new(Self::new(offset))
    }
}

#[typetag::serde]
impl ConditionSource for OffsetGroundLevelCondition {
    #[inline(never)]
    fn test(self: &Self, pos: &IVec3, context: &Context) -> bool {
        pos.y <= context.ground_y && pos.y > context.ground_y - self.offset
    }
}

/// Ground Y level condition with an offset. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainCondition {
    conditions: Vec<Box<ConditionSrc>>
}
impl ChainCondition {
    /// Helper for creating new `OffsetGroundLevelCondition`s
    pub fn new(conditions: Vec<Box<ConditionSrc>>) -> Self {
        Self {
            conditions: conditions
        }
    }

    /// Boxes a new `ChainCondition` automatically
    pub fn new_boxed(conditions: Vec<Box<ConditionSrc>>) -> Box<Self> {
        Box::new(Self::new(conditions))
    }
}

#[typetag::serde]
impl ConditionSource for ChainCondition {
    #[inline(never)]
    fn test(self: &Self, pos: &IVec3, context: &Context) -> bool {
        self.conditions.iter().all(|x| x.test(pos, context))
    }
}