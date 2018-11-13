// mod.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

mod stable;

#[cfg(feature = "unstable")]
mod unstable;

#[cfg(feature = "unstable")]
pub use self::unstable::*;

#[cfg(not(feature = "unstable"))]
pub use self::stable::*;

#[cfg(not(all(feature = "unstable", feature = "numa")))]
impl BitSet {
    pub fn on_node(self, node_id: u32) -> Self { self }
}