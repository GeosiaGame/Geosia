//! Default Rule Source types.

use serde::{Serialize, Deserialize};

use crate::voxel::biome::{RuleSrc, ConditionSrc};
use crate::voxel::voxeltypes::BlockEntry;

use super::RuleSource;

/// Empty Rule source. Does nothing.
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct EmptyRuleSource();

#[typetag::serde]
impl RuleSource for EmptyRuleSource {
    #[inline(never)]
    fn place(self: &Self, _pos: &bevy_math::IVec3, _context: &super::Context, _block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Rule source that runs `result`, but only if `condition` succeeds.
pub struct ConditionRuleSource {
    /// The condition to test for.
    condition: Box<ConditionSrc>,
    /// The Function to run.
    result: Box<RuleSrc>,
}

impl ConditionRuleSource {
    /// Helper for creating new `ConditionRuleSource`s
    pub fn new(condition: Box<ConditionSrc>, result: Box<RuleSrc>) -> Self {
        Self {
            condition: condition,
            result: result
        }
    }

    /// Boxes a new `ConditionRuleSource` automatically
    pub fn new_boxed(condition: Box<ConditionSrc>, result: Box<RuleSrc>) -> Box<Self> {
        Box::new(Self::new(condition, result))
    }
}

#[typetag::serde]
impl RuleSource for ConditionRuleSource {
    #[inline(never)]
    fn place(self: &Self, pos: &bevy_math::IVec3, context: &super::Context, block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        if self.condition.test(pos, context) {
            self.result.place(pos, context, block_registry)
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Rule source that runs `result`s until one returns non-empty.
pub struct ChainRuleSource {
    /// The Function(s) to run.
    rules: Vec<Box<RuleSrc>>,
}

impl ChainRuleSource {
    /// Helper for creating new `ChainRuleSource`s
    pub fn new(rules: Vec<Box<RuleSrc>>) -> Self {
        Self {
            rules: rules
        }
    }

    /// Boxes a new `ChainRuleSource` automatically
    pub fn new_boxed(rules: Vec<Box<RuleSrc>>) -> Box<Self> {
        Box::new(Self::new(rules))
    }
}

#[typetag::serde]
impl RuleSource for ChainRuleSource {
    #[inline(never)]
    fn place(self: &Self, pos: &bevy_math::IVec3, context: &super::Context, block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        for rule in self.rules.iter() {
            let result = rule.place(pos, context, block_registry);
            if result.is_some() {
                return result;
            }
        }
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Rule source that returns a BlockEntry.
pub struct BlockRuleSource {
    /// The `BlockEntry` this returns.
    entry: BlockEntry,
}

impl BlockRuleSource {
    /// Helper for creating new `BlockRuleSource`s
    pub fn new(entry: BlockEntry) -> Self {
        Self {
            entry: entry
        }
    }

    /// Boxes a new `ChainRuleSource` automatically
    pub fn new_boxed(entry: BlockEntry) -> Box<Self> {
        Box::new(Self::new(entry))
    }
}

#[typetag::serde]
impl RuleSource for BlockRuleSource {
    #[inline(never)]
    fn place(self: &Self, _pos: &bevy_math::IVec3, _context: &super::Context, _block_registry: &crate::voxel::voxeltypes::BlockRegistry) -> Option<BlockEntry> {
        Some(self.entry)
    }
}
