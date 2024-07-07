# The network protocol.
@0xb89146b09fd226cb;

using Rust = import "rust.capnp";
$Rust.parentModule("schemas");

using GameTypes = import "game_types.capnp";

# The main RPC entrypoint for the game server, (Anonymous client)->Server RPC.
interface GameServer @0xf0320743e0d6201d {
    struct Metadata @0xe9422344c157116e {
        serverVersion @0 :GameTypes.Version;
        title @1 :Text;
        subtitle @2 :Text;
        # Number of online players
        playerCount @3 :Int32;
        # Limit of online players (can be bypassed by administrators and moderators depending on settings)
        playerLimit @4 :Int32;
    }

    # Gets the server metadata.
    getServerMetadata @0 () -> (metadata: Metadata);
    # Returns the given number.
    ping @1 (input: Int32) -> (output: Int32);
    # Attempts to authenticate the connection in order to join as a player.
    authenticate @2 (username: Text, connection: AuthenticatedClientConnection) -> (conn: GameTypes.Result(AuthenticatedServerConnection, AuthenticationError));
}

struct AuthenticationError @0x9ed4d9765d345c1e {
    enum Kind @0x8a27ac929250061a {
        unspecifiedError @0;
        invalidUsername @1;
        serverFull @2;
        banned @3;
    }
    kind @0 :Kind;
    message @1 :Text;
}

# A stream startup message, determining the type of the stream.
# Sent as a LEB128-encoded data length + the encoded data array on a fresh QUIC stream.
# A single packet on an asynchronous stream is sent as a LEB128-encoded data length + the encoded data array.
# The data arrays are compressed on network sockets, and the capnp unpacked encoding is used.
struct StreamHeader {
    enum StandardTypes {
        chunkData @0;
    }
    # The stream type, used to determine the handler used for the packets afterwards.
    union {
        standardType @0 :StandardTypes;
        customType @1 :GameTypes.RegistryName;
    }
}

# Server->Client RPC interface
interface AuthenticatedClientConnection @0xddd4c8ca33d42019 {
    # Graceful connection shutdown.
    terminateConnection @0 (reason: ConnectionTermination) -> ();
    # Notifies the client about a chat message sent on the specified game tick.
    addChatMessage @1 (tick: UInt64, text: Text) -> ();

    struct ConnectionTermination @0xc64a369add9cb286 {
        enum Kind @0xf72513a07b41b403 {
            shuttingDown @0;
            kick @1;
            ban @2;
        }
        kind @0 :Kind;
        message @1 :Text;
    }
}

# Client->Server RPC interface
interface AuthenticatedServerConnection @0xcc65c2f3643e6ae0 {
    # Gets the data needed to bootstrap a server connection.
    bootstrapGameData @0 () -> (data: GameTypes.GameBootstrapData);
    # Sends a chat message to the server.
    sendChatMessage @1 (text: Text) -> ();
}

struct ChunkDataStreamPacket {
    # Game tick on which this chunk was updated.
    tick @0 :UInt64;
    # Revision number of the chunk, used by MutWatcher deserialization.
    revision @1 :UInt64;
    # AbsChunkPos of the chunk.
    position @2 :GameTypes.IVec3;
    # Serialized chunk data.
    data @3 :GameTypes.FullChunkData;
}
