//! Actions that are generally triggered by clients and handled on the server.

use bevy_math::Vec3;

use crate::actions::BlockAction::{BreakBlock, PlaceBlock};
use crate::coordinates::AbsBlockPos;
use crate::schemas::CapnpExt;
use crate::schemas::game_types_capnp::{block_action, position_data};

/// Position data
#[derive(Debug, Copy, Clone)]
pub struct PositionData {
    /// Position in the world
    pub position: AbsBlockPos,
    /// offset from position
    pub offset: Vec3,
    /// look vector
    pub look: Vec3,
}

impl PositionData {
    /// writes this position data to the given builder
    pub fn to_builder(self, builder: &mut position_data::Builder<'_>) {
        self.position.write_to_message(&mut builder.reborrow().init_position());
        self.offset.write_to_message(&mut builder.reborrow().init_offset());
        self.look.write_to_message(&mut builder.reborrow().init_look());
    }
}

/// Block Action, for world interaction by placing or breaking blocks.
/// For the debug cam era only, should be replaced with a more generalized,
/// item-driven system once the player is implemented.
#[derive(Debug, Copy, Clone)]
pub enum BlockAction {
    /// Place a block
    PlaceBlock(),
    /// Break a block
    BreakBlock(),
}

impl BlockAction {
    /// writes this block action to the given builder
    pub fn to_builder(self, builder: &mut block_action::Builder<'_>) {
        match self {
            PlaceBlock() => {
                builder.reborrow().init_place_block().set_unused(());
            }
            BreakBlock() => {
                builder.reborrow().init_break_block().set_unused(());
            }
        }
    }
}
