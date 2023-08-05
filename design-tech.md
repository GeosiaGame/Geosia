# Technical design

## Overall architecture

- Designed for multiplayer: Client/Server split from the start
- Singleplayer should run a server in a separate thread, or even process, communicating via shared memory. This minimizes the risk that a crash in one could cause data corruption in the other one.
- All assets should be available on both sides: servers might need textures for online map generation, sounds to know their duration, etc. - and missing pieces just make it easier to make mistakes in e.g. mods.
- A lot of base game elements should be data-driven by default (with complex code fallbacks as an option): "compiled" down to efficient binary formats for quick loading times, support for hot reloading and interacting with 3rd party tools.
- Everything should be designed for concurrent and parallel processing to make use of multiple CPU cores. This means minimizing mutations, especially non-local data mutations, and splitting some operations into multiple phases with synchronization barriers between them.

## World representation and processing

- A savefile is made up of the following components from smallest to largest:
1. Universe - top-level container for everything
2. Planet
   - Options for shape (needs a decision):
     - Flat world
     - "Flat" world with X/Y/Z wrapping (at some point the world loops around in every direction, with the vertical part mirroring to show crossing the planet center)
     - "Flat" world mapped onto a sphere
     - Cube world (voxels form a real cube)
3. Biome
   - Usually ~1000 m across, to provide enough building space and space for interesting features to spawn
4. Chunk
   - 32³ cubic blocks (16 meters across in every dimension), it means you only need 3*5=15 bits to uniquely identify any block in a chunk
   - The main unit of world loading/unloading
   - Some chunk groups might be "detached" from the main voxel grid, forming moving contraptions
5. Block
   - 0.5m across in every dimension
   - Can be:
     - A standard material block of one of the predefined shapes
     - A custom complex block with a custom model and potentially logic
     - A "placeholder" block that makes sure the block space is reserved for an Entity managing it
6. Entity
   - Can be smaller or bigger than a block, or not have a physical form at all
   - Can house advanced logic and be capable of storing complex data
   - Can just be a static container for data or a dynamic construct interacting with the world
   - Can be locked to a specific group of blocks, or a free-roaming entity simulated by the physics engine
   - Prefab entities are provided by the engine for common tasks such as inventory storage, recipe processing - these will be heavily optimized

## Inventories

- Stored as part of entities
- Most items in the game should be "dumb": a simple single-ID no-metadata item that can be stored with an int
- An optional metadata value will be provided for items that need to store a single integer and don't need complex data attached
  - Can be used for durability, energy charge, etc.
  - Different items, even if related, should use different IDs (e.g. an iron plate and a bronze plate should be two different IDs)
- For complex items, complex data can be attached. This will be a byte array that can be used to keep structured data as the implementation sees fit, with the requirement to serialize/deserialize to well-formed MessagePack data.
  - The byte array form enables code to do cheap comparisons of different items by just scanning one array and not jumping across a tree of pointers
  - Significantly more complex data can always spawn an entity to use for storage
  - MessagePack serialization ensures the game and other mods can reflect into the data buffer as needed, see the Serialization section for details

## Serialization

- All the data formats used by the game are defined in the `lib/ocg_schemas` crate, in a way that makes them usable for internal and external tools
- On-disk storage
  - A strongly-typed SQlite database will be initially used to store all savefile data, if we ran into limitations this can be split into multiple files or even a custom format
  - Most game code should be completely storage-agnostic, allowing for the data to be easily switched to a different format in development if we see it becomes necessary
- Network packet format: [Cap'n proto](https://capnproto.org/) will be used as the packet encoding scheme
  - It provides well-defined, backwards- and forwards-compatible schemas for limited interoperability of older and newer clients and servers
  - The schemas are language-agnostic, so can be used to create packet inspection tools in other languages, or server administration utilities
  - [Capabilities](https://en.wikipedia.org/wiki/Capability-based_security) offer a convenient and secure way to grant access to server-side objects on the client and vice versa

## Networking

- [QUIC](https://en.wikipedia.org/wiki/QUIC) is used as the network protocol of choice: it's based on UDP and avoids many TCP issues such as: head-of-line-blocking, inability to send unreliable messages, concurrent stream support
- A "main" stream can be used for most game interactions, with additional concurrent streams for high-bandwidth loads such as chunk streaming
- Datagrams can be used to send low-priority updates to data that changes frequently, if the old version doesn't need to be re-transmitted in case of packet loss

## Rendering

- A modular pipeline making use of bevy's modular rendering architecture
- Chunk rendering:
  - For each chunk, the vertex data is generated into three buffers: static data, "far" LoD data and "near" LoD data.
  - Static and "far" data is intended to rarely change and be efficient to render
  - "Near" data can contain dynamic elements and high-detail models, and is only rendered if the chunk is big enough on your screen to save GPU resources
- Block models
  - There is a set of standard static shape meshes for a block that the engine can generate into the static buffer
  - Custom meshes are supported
  - Faces invisible due to blocks touching each other are culled as much as possible during mesh construction to minimize the vertex buffer sizes
  - A block-tied entity can reserve a certain number of vertices in dynamic draw data and upload updates to the pre-allocated region to avoid needing to reconstruct the entire chunk when only that sub-mesh changes
- Non-block entity rendering
  - Entities of the same model get batched together and render using instancing to improve performance and save memory usage
  - Entities can output either a static, semi-dynamic or completely dynamic mesh with a list of drawcalls needed to render it
  - Drawcalls are [sorted](https://archive.is/vmApb) to ensure good performance and minimum GPU state switching

## Physics

- Idea: use [Rapier](https://rapier.rs/) as a powerful physics engine
  - Construct static collision meshes per chunk (and dynamic ones for contraptions)
  - This allows for complex moving contraptions
  - Handles broad-phase and narrow-phase collision detection fairly efficiently

## UI

- Must support keyboard&mouse, should support gamepad interactions

## Audio

## Modding

- An embedded WebAssembly runtime will provide a secure sandbox for mod code to run in
- Libraries for mods in different languages will be provided to make mod coding easier
