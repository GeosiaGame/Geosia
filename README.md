# OpenCubeGame

## Folder structure

Follows https://matklad.github.io/2021/08/22/large-rust-workspaces.html

- lib: Crates publishable to crates.io
  - ocg_schemas: Data type definitions for the on-disk, network and in-memory storage formats
- crates: Internal crates not intended for other projects to use
  - ocg_common: Code common to the client&server
  - ocg_client: The game client
- assets: Resources (textures, fonts, etc.)

## Design documents

- [Gameplay Design](./design-game.md)
- [Technical/Engine Design](./design-tech.md)

## Useful tools for development

- [Cap'n proto compiler]((https://capnproto.org/install.html): (the system package is usually called capnp). Used for compiling network protocol and disk storage schemas
- [Vulkan SDK](https://vulkan.lunarg.com/#new_tab): Provides validation layers, shader debugging tools and other useful utilities
- [RenderDoc](https://renderdoc.org/): can record full replayable GPU traces and visually inspect any rendering command
- [tracy](https://github.com/wolfpld/tracy): nanosecond-resolution interactive profiler, useful for identifying performance issues
