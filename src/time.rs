// time.rs Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use std::time::Instant;

#[cfg(feature = "unstable")]
pub fn get_elapsed_time(start: Instant) -> String {
    return format!("{} ms", start.elapsed().as_millis());
}

#[cfg(not(feature = "unstable"))]
pub fn get_elapsed_time(start: Instant) -> String {
    return format!("{} sec", start.elapsed().as_secs());
}