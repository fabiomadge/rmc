// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// @expect verified

fn double(a: u32) -> u32 {
    a * 2
}

pub fn main() {
    let a = rmc::any();
    if a <= std::u32::MAX / 2 {
        let b = double(a);
        assert!(b == 2 * a);
    }
}
