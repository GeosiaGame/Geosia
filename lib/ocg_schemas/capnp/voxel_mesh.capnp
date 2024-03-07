# Voxel mesh serialization format for resource packs.
@0xd51eb866f746ae4e;

using Rust = import "/rust.capnp";
$Rust.parentModule("schemas");

using GameTypes = import "/game_types.capnp";

# Encoding for "simple" voxel meshes that are used for the majority of blocks.

struct VoxelMesh @0xcb2b68862e89c245 {
    # Registry name of the mesh.
    name @0 :GameTypes.RegistryName;
    # How strongly each corner occludes light for ambient occlusion calculations, between 0 and 1.
    # Index 0 is for corner with X=0, Y=0, Z=0, then 001, 010, 011, etc.
    # The length of the list should be 8, if shorter it defaults to 0.0, if longer it's ignored.
    ambientOcclusionStrength @1 :List(Float32) = [ 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0 ];
    # The static mesh data, indexed by Direction for culling purposes, with side "6" never being culled by adjacent faces
    staticSides @2 :List(VoxelMeshSide);
}

struct VoxelMeshSide @0x9fcc5dd96c3d385c {
    # Whether this side covers the entire voxel area in this direction, stopping the neighbor from rendering
    canClip @0 :Bool;
    # Whether this side is within the voxel area in this direction, so it is possible that the neighbor is stopping it from rendering
    canBeClipped @1 :Bool;
    # The vertices of the mesh.
    vertices @2 :List(VoxelMeshVertex);
    # Indices into the vertices array, in triples forming triangles.
    indices @3 :List(UInt16);
}

struct VoxelMeshVertex @0x9f28cc7ada3ebd36 {
    # The offset from the center of the block.
    offsetX @0 :Float32;
    offsetY @1 :Float32;
    offsetZ @2 :Float32;
    # Texture mapping metadata
    textureLayer @3 :Int32;
    textureU @4 :Float32;
    textureV @5 :Float32;
    # A normal unit vector pointing "up" from the face this vertex is a part of
    normalX @6 :Float32;
    normalY @7 :Float32;
    normalZ @8 :Float32;
    # Barycentric coordinates within a face used for blending corrections
    # See https://www.asawicki.info/news_1721_how_to_correctly_interpolate_vertex_attributes_on_a_parallelogram_using_modern_gpus
    barycentricX @9 :Float32;
    barycentricY @10 :Float32;
    barycentricZ @11 :Float32;
    # Sign when added to the "extra data" sum for proper quadrilateral interpolation
    barycentricSign @12 :Int32;
    # List of voxel neighbors to take into account when calculating ambient occlusion
    ambientOcclusionNeighbors @13 :List(GameTypes.IVec3);
}
