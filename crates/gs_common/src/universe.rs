//! Types for existing in the physical universe of Geosia

use gs_schemas::coordinates::WorldPos;

use crate::prelude::*;

/// The Bevy plugin registering all the relevant types and systems
pub fn geosia_universe_plugin(app: &mut App) {
    app.insert_resource(UniverseTransformReference::default());
    app.add_systems(
        PostUpdate,
        sync_transforms
            .before(bevy::transform::systems::mark_dirty_trees)
            .in_set(TransformSystems::Propagate),
    );
}

/// Extension of [`Transform`] that stores a chunk + position offset for full precision anywhere in the universe.
///
/// Applying this transform automatically derives a matching [`Transform`] and [`GlobalTransform`] on the entity.
#[derive(Default, Debug, PartialEq, Clone, Copy, Component)]
#[require(Transform)]
pub struct UniverseTransform {
    /// This supersedes [`Transform`]'s `translation`
    pub position: WorldPos,
}

/// The reference point for what to subtract from [`UniverseTransform`]s to get the corresponding [`Transform`] positions.
#[derive(Default, Debug, PartialEq, Clone, Copy, Resource)]
pub struct UniverseTransformReference {
    /// This world position is treated as zero for bevy [`Transform`]s
    pub zero_position: WorldPos,
}

/// The system for syncing [`UniverseTransform`]s and [`Transform`]s
pub fn sync_transforms(
    reference: Res<UniverseTransformReference>,
    mut query: ParamSet<(
        Query<(&UniverseTransform, &mut Transform)>,
        Query<(&UniverseTransform, &mut Transform), Or<(Changed<UniverseTransform>, Added<Transform>)>>,
    )>,
) {
    let ref_pos = reference.zero_position;
    let update_fn = move |(universe_transform, mut transform): (&UniverseTransform, Mut<Transform>)| {
        transform.translation = (universe_transform.position - ref_pos).as_vec3();
    };
    if reference.is_changed() {
        query.p0().par_iter_mut().for_each(update_fn);
    } else {
        query.p1().par_iter_mut().for_each(update_fn);
    }
}
