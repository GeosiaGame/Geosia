# The network protocol.
@0xb89146b09fd226cb;

using Rust = import "rust.capnp";
$Rust.parentModule("schemas");

using GameTypes = import "game_types.capnp";

# Packet types, with payload types documented in comments
# C->S - client can request, S->C - server can request, c->S - client can only reply, s->C - server can only reply
# Each side opens its own bidi stream when first needed, and only sends requests on that stream, while replies with responses are sent to the stream opened by the other side.
# Additional bidi streams can be opened for sending additional packets asynchronously, when ordering is not important.
enum PacketId @0xb9187b435a666525 {
    # --- Unauthenticated packets ---

    # C->S :Int32
    # S->C :Int32 (same number)
    echo @0;

    # C->S :Bool (0/1 Int32) (whether to immediately shut down the connection after the response, available in 0-RTT for fast server status queries)
    # s->C :GameServerMetadata (timestamp is set to zero to allow for blind response copying)
    getServerMetadata @1;

    # C->S :AuthenticationRequest
    # s->C :Result(AuthenticationAcknowledgement, AuthenticationError)
    authenticate @2;

    # --- Authenticated packets ---

    # S->C :GameTypes.GameBootstrapData
    # C->S :Void (just an ACK)
    bootstrapGameData @3;

    # C->S :Text (server will reply back with the same message but formatted with the nickname, or a rejection message to display in chat)
    # S->C :Text
    chatMessage @4;

    # C->S :BlockActionRequest
    # S->C :Int32 SimpleResult
    blockAction @5;

    # S->C :ChunkDataStreamPacket (usually asynchronous)
    chunkData @6;
}

# Each packet is prefixed with a LEB128-encoded length field
struct NetworkPacket @0xc4766635d464f2d0 (PayloadType) {
    id @0 :PacketId;
    # Measured since an arbitrary time point in milliseconds
    timestampMs @1 :UInt64;
    payload @2 :PayloadType;
    simplePayload @3 :Int32;
}

# QUIC CONNECTION_CLOSE reason contents
struct ConnectionTermination @0xc64a369add9cb286 {
    enum Kind @0xf72513a07b41b403 {
        shuttingDown @0;
        kick @1;
        ban @2;
    }
    kind @0 :Kind;
    message @1 :Text;
}

struct GameServerMetadata @0xe9422344c157116e {
    serverVersion @0 :GameTypes.Version;
    title @1 :Text;
    subtitle @2 :Text;
    # Number of online players
    playerCount @3 :Int32;
    # Limit of online players (can be bypassed by administrators and moderators depending on settings)
    playerLimit @4 :Int32;
}

struct AuthenticationRequest @0xc139dbbb639799f2 {
    playerUrl @0 :Text;
    playerDisplayName @1 :Text;
    characterId @2 :GameTypes.Uuid;
    characterDisplayName @3 :Text;
    token @4 :Text;
}

struct AuthenticationAcknowledgement @0xb0d8fc5025c40234 {
}

struct AuthenticationError @0x9ed4d9765d345c1e {
    enum Kind @0x8a27ac929250061a {
        unspecifiedError @0;
        invalidProfile @1;
        serverFull @2;
        banned @3;
        alreadyAuthenticated @4;
    }
    kind @0 :Kind;
    message @1 :Text;
}

struct BlockActionRequest @0xa74244e28dcb5f2f {
    position @0 :GameTypes.PositionData;
    action @1 :GameTypes.BlockAction;
    tick @2 :UInt64;
}

struct ChunkDataStreamPacket {
    # Game tick on which this chunk was updated.
    tick @0 :UInt64;
    # AbsChunkPos of the chunk.
    position @1 :GameTypes.IVec3;
    # Serialized chunk data.
    data @2 :GameTypes.FullChunkData;
}
