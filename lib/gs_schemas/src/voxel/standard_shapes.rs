//! Standard voxel shape meshes and metadata implementation
//! TODO: Replace with a flexible system that can accept user data
use bevy_math::prelude::*;
use bevy_math::Vec3A;
use once_cell::sync::Lazy;
use smallvec::{smallvec, SmallVec};

use crate::direction::OctahedralOrientation;
use crate::voxel::voxeltypes::BlockMetadata;

/// Helper for determining the shape&orientation of a standard-shaped block from its metadata.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct StandardShapeMetadata {
    shape: u16,
    orientation: u16,
}

impl StandardShapeMetadata {
    /// A default metadata object for a meta value of 0.
    pub fn new() -> Self {
        Default::default()
    }

    /// Construct standard shape metadata from the given shape and orientation IDs.
    pub fn from_parts(shape: u16, orientation: u16) -> Option<Self> {
        if shape >= 64 || orientation >= 24 {
            None
        } else {
            Some(Self { shape, orientation })
        }
    }

    /// Constructs standard shape metadata from the given block metadata value.
    pub fn from_meta(meta: BlockMetadata) -> Self {
        Self {
            shape: (meta & 0xFFFF) as u16,
            orientation: ((meta >> 16) & 0xFFFF) as u16,
        }
    }

    /// Gets the equivalent block metadata value.
    pub fn to_meta(self) -> BlockMetadata {
        (((self.orientation as u32) << 16) | self.shape as u32) as BlockMetadata
    }

    /// Gets the raw shape ID.
    pub fn shape_bits(self) -> u16 {
        self.shape
    }

    /// Gets the raw orientation ID.
    pub fn orientation_bits(self) -> u16 {
        self.orientation
    }

    /// Gets the voxel shape definition object corresponding to this metadata value.
    pub fn shape(self) -> &'static VoxelShapeDef {
        match self.shape_bits() {
            STANDARD_SHAPE_CUBE => &VOXEL_CUBE_SHAPE,
            STANDARD_SHAPE_SLOPE => &VOXEL_SLOPE_SHAPE,
            STANDARD_SHAPE_CORNER => &VOXEL_CORNER_SHAPE,
            STANDARD_SHAPE_INNER_CORNER => &VOXEL_INNER_CORNER_SHAPE,
            _ => &VOXEL_CUBE_SHAPE,
        }
    }

    /// Gets the orientation corresponding to this metadata value.
    pub fn orientation(self) -> OctahedralOrientation {
        OctahedralOrientation::try_from_index(self.orientation_bits() as usize).unwrap_or_default()
    }
}

/// The shape ID for a cube.
pub const STANDARD_SHAPE_CUBE: u16 = 0;
/// The shape ID for a slope.
pub const STANDARD_SHAPE_SLOPE: u16 = 1;
/// The shape ID for a corner slope.
pub const STANDARD_SHAPE_CORNER: u16 = 2;
/// The shape ID for a inner corner slope.
pub const STANDARD_SHAPE_INNER_CORNER: u16 = 3;

/// A definition of a voxel mesh.
#[derive(Clone, Debug)]
pub struct VoxelShapeDef {
    /// Whether this mesh produces ambient occlusion shadows around it.
    pub causes_ambient_occlusion: bool,
    /// The submeshes for each of the mesh sides, indexed by [`crate::direction::Direction`] IDs.
    pub sides: [VSSide; 6],
}

/// A vertex for a voxel mesh.
#[derive(Clone, Debug)]
pub struct VSVertex {
    /// The offset from the center of the block.
    pub offset: Vec3A,
    /// The texture mapping coordinates.
    pub texcoord: Vec2,
    /// A unit length vector pointing "up" from the face it's a part of.
    pub normal: Vec3A,
    /// Barycentric coordinate within the voxel face.
    /// <https://www.asawicki.info/news_1721_how_to_correctly_interpolate_vertex_attributes_on_a_parallelogram_using_modern_gpus>
    /// Archive: <https://web.archive.org/web/20200516133048/https://www.asawicki.info/news_1721_how_to_correctly_interpolate_vertex_attributes_on_a_parallelogram_using_modern_gpus>
    pub barycentric: Vec3A,
    /// Sign when added to the "extra data" sum for proper quadrilateral interpolation
    pub barycentric_sign: i32,
    /// List of blocks to check to calculate the ambient occlusion values.
    pub ao_offsets: SmallVec<[IVec3; 4]>,
}

/// A single side of a voxel mesh.
#[derive(Clone, Debug)]
pub struct VSSide {
    /// Whether this side covers the entire voxel area in this direction, stopping the neighbor from rendering
    pub can_clip: bool,
    /// Whether this side is within the voxel area in this direction, so it is possible that the neighbor is stopping it from rendering
    pub can_be_clipped: bool,
    /// The vertices of the mesh.
    pub vertices: SmallVec<[VSVertex; 8]>,
    /// Indices into the vertices array, in triples forming triangles.
    pub indices: SmallVec<[u32; 8]>,
}

/// Shape definition for empty voxels.
pub static VOXEL_NO_SHAPE: Lazy<VoxelShapeDef> = Lazy::new(init_no_shape);
/// Shape definitions for cube blocks.
pub static VOXEL_CUBE_SHAPE: Lazy<VoxelShapeDef> = Lazy::new(init_cube_shape);
/// Shape definitions for slope blocks.
pub static VOXEL_SLOPE_SHAPE: Lazy<VoxelShapeDef> = Lazy::new(init_slope_shape);
/// Shape definitions for corner slope blocks.
pub static VOXEL_CORNER_SHAPE: Lazy<VoxelShapeDef> = Lazy::new(init_corner_shape);
/// Shape definitions for inner corner slope blocks.
pub static VOXEL_INNER_CORNER_SHAPE: Lazy<VoxelShapeDef> = Lazy::new(init_inner_corner_shape);

fn init_no_shape() -> VoxelShapeDef {
    let side = VSSide {
        can_clip: false,
        can_be_clipped: true,
        vertices: SmallVec::new(),
        indices: SmallVec::new(),
    };
    VoxelShapeDef {
        causes_ambient_occlusion: false,
        sides: [
            side.clone(),
            side.clone(),
            side.clone(),
            side.clone(),
            side.clone(),
            side,
        ],
    }
}

/// A signum function that returns 0 for values x where |x|<0.1
fn approx_signum(mut v: Vec3A) -> IVec3 {
    let abs = v.abs();
    if abs.x < 0.1 {
        v.x = 0.0;
    }
    if abs.y < 0.1 {
        v.y = 0.0;
    }
    if abs.z < 0.1 {
        v.z = 0.0;
    }
    v.signum().as_ivec3()
}

/// Calculates the set of ambient occlusion neighbors from the position&normal at a given vertex.
fn corner_ao_set(corner: Vec3A, inormal: IVec3) -> SmallVec<[IVec3; 4]> {
    let icorner = approx_signum(corner);
    let mut sv = SmallVec::new();
    sv.push(icorner);
    sv.push(inormal);
    if inormal.x == 0 {
        let mut c = icorner;
        c.x = 0;
        sv.push(c);
    }
    if inormal.y == 0 {
        let mut c = icorner;
        c.y = 0;
        sv.push(c);
    }
    if inormal.z == 0 {
        let mut c = icorner;
        c.z = 0;
        sv.push(c);
    }
    sv
}

/// Constructs a list of vertices for a quad with the given center and local right&up vectors.
fn quad_verts(center: Vec3A, right: Vec3A, up: Vec3A) -> SmallVec<[VSVertex; 8]> {
    let fnormal = -right.cross(up);
    let inormal = approx_signum(fnormal);
    let fnormal = fnormal.normalize();
    smallvec![
        VSVertex {
            offset: center - right - up,
            texcoord: Vec2::new(0.0, 1.0),
            normal: fnormal,
            barycentric: Vec3A::new(0.0, 1.0, 1.0),
            barycentric_sign: -1,
            ao_offsets: corner_ao_set(center - right - up, inormal),
        },
        VSVertex {
            offset: center - right + up,
            texcoord: Vec2::new(0.0, 0.0),
            normal: fnormal,
            barycentric: Vec3A::new(0.0, 0.0, 1.0),
            barycentric_sign: 1,
            ao_offsets: corner_ao_set(center - right + up, inormal),
        },
        VSVertex {
            offset: center + right + up,
            texcoord: Vec2::new(1.0, 0.0),
            normal: fnormal,
            barycentric: Vec3A::new(1.0, 0.0, 1.0),
            barycentric_sign: -1,
            ao_offsets: corner_ao_set(center + right + up, inormal),
        },
        VSVertex {
            offset: center + right - up,
            texcoord: Vec2::new(1.0, 1.0),
            normal: fnormal,
            barycentric: Vec3A::new(0.0, 0.0, 1.0),
            barycentric_sign: 1,
            ao_offsets: corner_ao_set(center + right - up, inormal),
        },
    ]
}

const QUAD_INDICES: [u32; 6] = [0, 1, 2, 2, 3, 0];

fn init_cube_shape() -> VoxelShapeDef {
    VoxelShapeDef {
        causes_ambient_occlusion: true,
        sides: [
            // Left X-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(-0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, -0.5),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Right X+
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, 0.5),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Bottom Y-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, -0.5, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, -0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Top Y+
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.5, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, 0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Front Z-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.0, -0.5),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Back Z+
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.0, 0.5),
                    Vec3A::new(-0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
        ],
    }
}

fn init_slope_shape() -> VoxelShapeDef {
    VoxelShapeDef {
        causes_ambient_occlusion: true,
        sides: [
            // Left X-
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, 1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, -0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, -1.0), IVec3::new(-1, 0, 0)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
            // Right X+
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, -0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, -1.0), IVec3::new(1, 0, 0))
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, 0.5, 0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, 1.0, 1.0), IVec3::new(1, 0, 0))
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, 0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, 1.0), IVec3::new(1, 0, 0))
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
            // Bottom Y-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, -0.5, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, -0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Top Y+
            VSSide {
                can_clip: false,
                can_be_clipped: false,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.0, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.5, 0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Front Z-
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![],
                indices: SmallVec::new(),
            },
            // Back Z+
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.0, 0.5),
                    Vec3A::new(-0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
        ],
    }
}

fn init_corner_shape() -> VoxelShapeDef {
    VoxelShapeDef {
        causes_ambient_occlusion: true,
        sides: [
            // Left X-
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, 1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, -0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, -1.0), IVec3::new(-1, 0, 0)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
            // Right X+
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![],
                indices: smallvec![],
            },
            // Bottom Y-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, -0.5, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, -0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Top Y+
            VSSide {
                can_clip: false,
                can_be_clipped: false,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, -0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(0.0, 1.0, -1.0).normalize(),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, -1.0), IVec3::new(0, 0, -1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, -0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(0.0, 1.0, -1.0).normalize(),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, -1.0), IVec3::new(0, 0, -1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(0.0, 1.0, -1.0).normalize(),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, 1.0), IVec3::new(0, 1, 0)),
                    },
                    //
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, 0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, 1.0), IVec3::new(0, 1, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, 0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, 1.0), IVec3::new(1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, -0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, -1.0), IVec3::new(1, 0, 0)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2, 3, 4, 5]),
            },
            // Front Z-
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![],
                indices: smallvec![],
            },
            // Back Z+
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, 0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
        ],
    }
}

fn init_inner_corner_shape() -> VoxelShapeDef {
    VoxelShapeDef {
        causes_ambient_occlusion: true,
        sides: [
            // Left X-
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, -0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, -1.0), IVec3::new(-1, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, -0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(-1.0, 0.0, 0.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, -1.0), IVec3::new(-1, 0, 0)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
            // Right X+
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, 0.5),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Bottom Y-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, -0.5, 0.0),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.0, -0.5),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Top Y+
            VSSide {
                can_clip: false,
                can_be_clipped: false,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(0.5, 0.5, -0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(0.0, 1.0, 1.0).normalize(),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, 1.0, -1.0), IVec3::new(0, 1, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, 0.5, -0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(0.0, 1.0, 1.0).normalize(),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, 1.0, -1.0), IVec3::new(0, 1, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(1.0, 1.0),
                        normal: Vec3A::new(0.0, 1.0, 1.0).normalize(),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(0, 0, 0)),
                    },
                    //
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(-1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(0, 0, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, 0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(-1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, 1.0, 1.0), IVec3::new(0, 1, 0)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, 0.5, -0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(-1.0, 1.0, 0.0).normalize(),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, 1.0, -1.0), IVec3::new(0, 1, 0)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2, 3, 4, 5]),
            },
            // Front Z-
            VSSide {
                can_clip: true,
                can_be_clipped: true,
                vertices: quad_verts(
                    Vec3A::new(0.0, 0.0, -0.5),
                    Vec3A::new(0.5, 0.0, 0.0),
                    Vec3A::new(0.0, 0.5, 0.0),
                ),
                indices: SmallVec::from_slice(&QUAD_INDICES),
            },
            // Back Z+
            VSSide {
                can_clip: false,
                can_be_clipped: true,
                vertices: smallvec![
                    VSVertex {
                        offset: Vec3A::new(0.5, 0.5, 0.5),
                        texcoord: Vec2::new(0.0, 1.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, 1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(-0.5, -0.5, 0.5),
                        texcoord: Vec2::new(1.0, 0.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(1.0, 0.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(-1.0, -1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                    VSVertex {
                        offset: Vec3A::new(0.5, -0.5, 0.5),
                        texcoord: Vec2::new(0.0, 0.0),
                        normal: Vec3A::new(0.0, 0.0, 1.0),
                        barycentric: Vec3A::new(0.0, 1.0, 0.0),
                        barycentric_sign: 0,
                        ao_offsets: corner_ao_set(Vec3A::new(1.0, -1.0, 1.0), IVec3::new(0, 0, 1)),
                    },
                ],
                indices: SmallVec::from_slice(&[0, 1, 2]),
            },
        ],
    }
}
