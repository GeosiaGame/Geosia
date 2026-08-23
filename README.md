# Geosia

## Folder structure

Follows https://matklad.github.io/2021/08/22/large-rust-workspaces.html

- lib: Crates publishable to crates.io
  - gs_schemas: Data type definitions for the on-disk, network and in-memory storage formats
- crates: Internal crates not intended for other projects to use
  - gs_common: Code common to the client&server
  - gs_client: The game client
- assets: Resources (textures, fonts, etc.)

## Design documents

- [Gameplay Design](./design-game.md)
- [Technical/Engine Design](./design-tech.md)

## Useful tools for development

- [Cap'n proto compiler]((https://capnproto.org/install.html): (the system package is usually called capnp). Used for compiling network protocol and disk storage schemas
- [Vulkan SDK](https://vulkan.lunarg.com/#new_tab): Provides validation layers, shader debugging tools and other useful utilities
- [RenderDoc](https://renderdoc.org/): can record full replayable GPU traces and visually inspect any rendering command
- [tracy](https://github.com/wolfpld/tracy): nanosecond-resolution interactive profiler, useful for identifying performance issues

## Hard limits

- 63 players connected to a server at any given time

# Contributing

## LLM policy

AI must not be used to generate code or assets for contributions to this project.

"AI" in this case means a Large Language Model ("LLM"), such as ChatGPT, Claude, Copilot, Grok, etc.

AI-generated code is based upon sources of unknown origins and may not be compatible with the LGPL-3.0-only license, or may introduce conflicting license terms if they include code from other projects.

AI can be used to identify issues with contributions to this project, but the solutions to those issues should be authored by humans.

We have found that AI will frequently hallucinate issues that are not actually problems in practice, report incorrect information, and describe problems that are actually not issues at all.
If AI identifies a problem with this codebase, please make sure you understand what it is saying and have independently confirmed that the issue exists before submitting a bug report or pull request.

Any pull request to this project will ask you to confirm that you are the author and that you are contributing your changes under the LGPL-3.0-only license.
