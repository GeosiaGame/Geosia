//! Actions that are generally triggered by clients and handled on the server.

use bevy_math::{IVec3, Vec3};
use crate::actions::ThrowAction::{ThrowBlock, ThrowItem};
use crate::coordinates::AbsBlockPos;
use crate::schemas::game_types_capnp::{position_data, throw_action};

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

impl TryFrom<position_data::Reader<'_>> for PositionData {
    type Error = ();

    fn try_from(value: position_data::Reader) -> Result<Self, Self::Error> {
        let position = {
            let pos = value.reborrow().get_position().map_err(|_| {()})?;
            AbsBlockPos { 0: IVec3 {
                x: pos.get_x(),
                y: pos.get_y(),
                z: pos.get_z()
            }}
        };
        let offset = {
            let offset = value.reborrow().get_offset().map_err(|_| {()})?;
            Vec3 {
                x: offset.get_x(),
                y: offset.get_y(),
                z: offset.get_z(),
            }
        };
        let look = {
            let look = value.reborrow().get_look().map_err(|_| {()})?;
            Vec3 {
                x: look.get_x(),
                y: look.get_y(),
                z: look.get_z(),
            }
        };
        Ok(PositionData { position, offset, look })
    }
}

impl PositionData {

    /// writes this position data to the given builder
    pub fn to_builder(self, builder: &mut position_data::Builder<'_>) {
        {
            let mut pos = builder.reborrow().init_position();
            pos.set_x(self.position.x);
            pos.set_y(self.position.y);
            pos.set_z(self.position.z);
        }
        {
            let mut offset = builder.reborrow().init_offset();
            offset.set_x(self.offset.x);
            offset.set_y(self.offset.y);
            offset.set_z(self.offset.z);
        }
        {
            let mut look = builder.reborrow().init_look();
            look.set_x(self.look.x);
            look.set_y(self.look.y);
            look.set_z(self.look.z);
        }
    }
}

/// Throw Action, for world interaction
#[derive(Debug, Copy, Clone)]
pub enum ThrowAction {
    ThrowBlock(),
    ThrowItem(),
}

impl TryFrom<throw_action::Reader<'_>> for ThrowAction {
    type Error = ();

    fn try_from(value: throw_action::Reader) -> Result<Self, Self::Error> {
        match value.reborrow().which() {
            Ok(throw_action::Which::ThrowItem(_)) => {
                Ok(ThrowItem())
            }
            Ok(throw_action::Which::ThrowBlock(_)) => {
                Ok(ThrowBlock())
            }
            _ => Err(())
        }
    }
}

impl ThrowAction {

    /// writes this throw action to the given builder
    pub fn to_builder(self, builder: &mut throw_action::Builder<'_>) {
        match self {
            ThrowBlock() => {
                builder.reborrow().init_throw_block().set_unused(());
            }
            ThrowItem() => {
                builder.reborrow().init_throw_item().set_unused(());
            }
        }
    }
}