# OpenCubeGame

## Folder structure

Follows https://matklad.github.io/2021/08/22/large-rust-workspaces.html

 - lib: Crates publishable to crates.io
   - ocg_schemas: Data type definitions for the on-disk, network and in-memory storage formats
 - crates: Internal crates not intended for other projects to use
   - ocg_common: Code common to the client&server
   - ocg_client: The game client
 - assets: Resources (textures, fonts, etc.)

