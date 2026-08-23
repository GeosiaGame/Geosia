//! Sets and documents hard limits on resources used by the game.

/// For network synchronization purposes, we need to keep track of a bit for each player in scenarios such as entity sync.
/// This could eat quite a bit of memory per-entity if it's too big, so a fixed-width bitset is used on top of a integer type right now.
/// In the future, this could be replaced with a smarter bitset type that does memory allocation beyond a certain number of players.
pub type ConnectedPlayersBitset = u64;

/// The limit on the number of players connected to a server at any given time, based off [`ConnectedPlayersBitset`].
/// The minus one simplifies some bitwise math (e.g. makes max+1 representable in the same type for generating bit masks).
pub const MAX_CONNECTED_PLAYERS: u32 = ConnectedPlayersBitset::BITS - 1;
