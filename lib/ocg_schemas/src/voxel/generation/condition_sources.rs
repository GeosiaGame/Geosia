//! Default Condition Source types.

use serde::{Serialize, Deserialize};

use crate::voxel::{generation::{ConditionSource, Context}, biome::ConditionSrc};

/// Always-true condition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlwaysTrueCondition();

#[typetag::serde]
impl ConditionSource for AlwaysTrueCondition {
    fn test(&mut self, _pos: bevy_math::IVec3, _context: &Context) -> bool {
        true
    }
}

/// Minimum Y level condition. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct YLevelCondition {
    min_y: i32
}

#[typetag::serde]
impl ConditionSource for YLevelCondition {
    fn test(&mut self, pos: bevy_math::IVec3, _context: &Context) -> bool {
        pos.y >= self.min_y
    }
}

/// Invert condition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotCondition {
    condition: Box<ConditionSrc>
}

#[typetag::serde]
impl ConditionSource for NotCondition {
    fn test(&mut self, pos: bevy_math::IVec3, context: &Context) -> bool {
        !self.condition.test(pos, context)
    }
}

/// Minimum ocena Y level condition. Inclusive.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroundLevelCondition();

#[typetag::serde]
impl ConditionSource for GroundLevelCondition {
    fn test(&mut self, pos: bevy_math::IVec3, context: &Context) -> bool {
        context.ground_y >= pos.y
    }
}