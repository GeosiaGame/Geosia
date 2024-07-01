//! A utility for dealing with a central voxel/chunk/etc. and all its neighbors (including diagonals)

use bevy_math::IVec3;
use itertools::iproduct;
use smallvec::SmallVec;

use crate::coordinates::AbsChunkPos;
use crate::mutwatcher::MutWatcher;
use crate::voxel::chunk::Chunk;

/// A reference to a 3³ cube of `Object`, useful for access to a chunk and its neighbors.
#[derive(Default, Clone, Eq, PartialEq)]
pub struct Neighborhood<Object, CoordType: From<IVec3> + Into<IVec3> + Copy> {
    center: CoordType,
    /// An XZY-strided array (index 13 is the central object).
    objects: [Object; 27],
}

impl<Object, CoordType: From<IVec3> + Into<IVec3> + Copy> Neighborhood<Object, CoordType> {
    /// Index of the central object in the data array.
    #[allow(clippy::identity_op)] // *1 used for clarity
    pub const CENTER_INDEX: usize = 3 * 3 * 1 + 3 * 1 + 1;

    /// Constructs a neighborhood from the location of the center and a mapping function of coordinates to values.
    pub fn from_center<CoordMapping: Fn(CoordType) -> Object>(
        center_position: CoordType,
        coord_fn: CoordMapping,
    ) -> Self {
        let mut out: SmallVec<[Object; 27]> = SmallVec::new();
        let center_raw: IVec3 = center_position.into();
        for (y, z, x) in iproduct!(0..3, 0..3, 0..3) {
            let pos_raw = center_raw + IVec3::new(x, y, z);
            out.push(coord_fn(pos_raw.into()));
        }
        Self {
            center: center_position,
            objects: out.into_inner().ok().unwrap(),
        }
    }

    /// An XZY-strided array of the objects in this neighborhood.
    pub fn objects_xzy(&self) -> &[Object; 27] {
        &self.objects
    }
    /// An XZY-strided array of the objects in this neighborhood.
    pub fn objects_xzy_mut(&mut self) -> &mut [Object; 27] {
        &mut self.objects
    }

    /// The coordinate of the center of the neighborhood.
    pub fn center_coord(&self) -> CoordType {
        self.center
    }

    /// The coordinate of the corner of the neighborhood with smallest coordinate values.
    pub fn min_coord(&self) -> CoordType {
        (self.center.into() - IVec3::splat(1)).into()
    }

    /// The coordinate of the corner of the neighborhood with largest coordinate values.
    pub fn max_coord(&self) -> CoordType {
        (self.center.into() + IVec3::splat(1)).into()
    }

    /// A reference to the central object.
    pub fn center(&self) -> &Object {
        &self.objects[Self::CENTER_INDEX]
    }

    /// A mutable reference to the central object.
    pub fn center_mut(&mut self) -> &Object {
        &mut self.objects[Self::CENTER_INDEX]
    }

    /// Calculates the index in the storage array of an object with the specific coordinates.
    /// Returns `None` if the coordinates are outside the neighborhood.
    pub fn index_of_coord(&self, coord: CoordType) -> Option<usize> {
        let offset = coord.into() - self.min_coord().into();
        if (offset.cmplt(IVec3::ZERO) | offset.cmpgt(IVec3::splat(2))).any() {
            None
        } else {
            Some((offset.x + 3 * offset.z + 3 * 3 * offset.y) as usize)
        }
    }

    /// A reference to the object at the given coordinates, or None if outside the neighborhood.
    pub fn get(&self, coord: CoordType) -> Option<&Object> {
        self.index_of_coord(coord).map(|idx| &self.objects[idx])
    }

    /// A reference to the object at the given coordinates, or None if outside the neighborhood.
    pub fn get_mut(&mut self, coord: CoordType) -> Option<&mut Object> {
        self.index_of_coord(coord).map(|idx| &mut self.objects[idx])
    }
}

impl<Object, CoordType: From<IVec3> + Into<IVec3> + Copy> Neighborhood<Option<Object>, CoordType> {
    /// Returns a `Some(n: Neighborhood<Object, CoordType>)` if all neighbors and the middle are present, or `None` if at least one is missing.
    pub fn transpose_option(self) -> Option<Neighborhood<Object, CoordType>> {
        let Self { center, objects } = self;
        let mut new_objects: SmallVec<[Object; 27]> = SmallVec::new();
        for obj in objects {
            new_objects.push(obj?);
        }
        Some(Neighborhood::<Object, CoordType> {
            center,
            objects: new_objects.into_inner().ok().unwrap(),
        })
    }
}

/// A neighborhood of optionally loaded chunks.
pub type OptionalChunkRefNeighborhood<'c, ExtraChunkData> =
    Neighborhood<Option<&'c MutWatcher<Chunk<ExtraChunkData>>>, AbsChunkPos>;
/// A mutable neighborhood of optionally loaded chunks.
pub type OptionalChunkRefMutNeighborhood<'c, ExtraChunkData> =
    Neighborhood<Option<&'c mut MutWatcher<Chunk<ExtraChunkData>>>, AbsChunkPos>;
/// A neighborhood of loaded chunks.
pub type ChunkRefNeighborhood<'c, ExtraChunkData> = Neighborhood<&'c MutWatcher<Chunk<ExtraChunkData>>, AbsChunkPos>;
/// A mutable neighborhood of loaded chunks.
pub type ChunkRefMutNeighborhood<'c, ExtraChunkData> =
    Neighborhood<&'c mut MutWatcher<Chunk<ExtraChunkData>>, AbsChunkPos>;
