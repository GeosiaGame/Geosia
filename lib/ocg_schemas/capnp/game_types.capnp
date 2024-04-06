# Common game data types exchanged over the wire.
@0xa5e994c4ed48b34c;

using Rust = import "/rust.capnp";
$Rust.parentModule("schemas");

struct Option @0x8ba1c86d2c77fb36 (T) {
    union {
        some @0 :T;
        none @1 :Void;
    }
}

struct Result @0x8f3f5259477b8021 (Ok, Err) {
    union {
        ok @0 :Ok;
        err @1 :Err;
    }
}

struct Version @0x838ff703b6e691c4 {
    # SemVer: https://semver.org/
    major @0 :UInt32;
    minor @1 :UInt32;
    patch @2 :UInt32;
    prerelease @3 :Text;
    build @4 :Text;
}

struct Uuid @0xad396073ac7dea91 {
    low @0 :UInt64;
    high @1 :UInt64;
}

# Simple math types
struct IVec2 @0x82293be591f48c52 {
    x @0 :Int32;
    y @1 :Int32;
}

struct IVec3 @0x8656ec7ddc60888e {
    x @0 :Int32;
    y @1 :Int32;
    z @2 :Int32;
}

struct I64Vec2 @0xf5a1c5adf2c416d4 {
    x @0 :Int64;
    y @1 :Int64;
    z @2 :Int64;
}

struct I64Vec3 @0x928035efab755766 {
    x @0 :Int64;
    y @1 :Int64;
    z @2 :Int64;
}

struct Vec2 @0xa6a42c0dee272cb9 {
    x @0 :Float32;
    y @1 :Float32;
}

struct Vec3 @0xed69b4c78460e1c0 {
    x @0 :Float32;
    y @1 :Float32;
    z @2 :Float32;
}

struct Quat @0xfab98c6be0936a7d {
    x @0 :Float32;
    y @1 :Float32;
    z @2 :Float32;
    w @3 :Float32;
}

# Named registry-related types
struct RegistryName @0xbbde17bbe6af716a {
    ns @0 :Text;
    key @1 :Text;
}

struct RegistryIdMappingBundle @0xe1c96c086209943d {
    # Inlined RegistryName for efficiency
    # All lists must have equal length
    nss @0 :List(Text);
    keys @1 :List(Text);
    ids @2 :List(UInt32); # NonZero
}

# The bootstrap data package to set up all client-side data for the connection.
struct GameBootstrapData @0xb0778941893c57e5 {
    # Stable UUID of the server's universe, used for identifying unique server "savefiles".
    universeId @0 :Uuid;
    # Name->ID mappings for the block registry.
    blockRegistry @1 :RegistryIdMappingBundle;
}

struct FullChunkData {
    blockPalette @0 :List(UInt64);
    blockData @1 :List(UInt16);
}
