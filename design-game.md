# Game Design

## One sentence pitch

OCG is a block-oriented sandbox factory game where you start your factory with almost nothing, and grow it to an interplanetary scale.

## Key aspects of the game

- Block-oriented: the game plays out on a voxel grid with a restricted set of shapes to build from.
- Sandbox: the player can modify any part of the game world if they progress enough through the technology tree
- Factory: every aspect of the game should be automatable, with the automation being enough of a puzzle to form its own gameplay loop
- Technology stages: to give a sense of progression, and define longer periods of time, technology eras are clusters of materials, machines and places accessible at specific stages of the gameplay
- Interplanetary: space-based logistics enter the game at a somewhat early stage to encourage exploration of dangerous environments and infrastructure building

## Building aspects of the game

- (0.5 m)³ cubes with a limited set of shapes, providing enough flexibility to build interesting structures, but enough restriction to force you to think creatively
- Basic building shapes:
  - Cubes
  - Slopes
  - Corner slopes
  - Inverted corner slopes
  - Rods (posts, rods, walls depending on the material)
- Most of the time you'll be working using tools that manipulate larger clusters of voxels (2³, 3³, 4³) for convenience
- Going down to single-block detail level should be easy
- You only need to hold the raw material in your inventory, and can pick a shape to build from it
- Hotkeys to switch between shaping using a terrain smoothing tool (approximated by slopes and cubes), building flat structures, lines and individual blocks

## Factory/Automation aspects

- Key requirement: the factory must look alive. See items moving around, energy conduits glowing, furnaces giving off a heat effect
- Some automation should be immediately available, with more convenient forms unlocking as more complex processes become necessary
- Most machines will be "multiblock": taking up the space of many blocks, often composed of various parts working in unison
