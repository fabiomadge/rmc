// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT
//
// Check that none of these operations trigger spurious overflow checks.
#![feature(unchecked_math)]

macro_rules! verify_no_overflow {
    ($cf: ident, $uf: ident) => {{
        let a: u8 = rmc::nondet();
        let b: u8 = rmc::nondet();
        let checked = a.$cf(b);
        rmc::assume(checked.is_some());
        let unchecked = unsafe { a.$uf(b) };
        assert!(checked.unwrap() == unchecked);
    }};
}

fn main() {
    verify_no_overflow!(checked_add, unchecked_add);
    verify_no_overflow!(checked_sub, unchecked_sub);
    verify_no_overflow!(checked_mul, unchecked_mul);
}
