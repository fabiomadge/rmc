// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0 OR MIT

use std::mem;

fn main() {
    let mut var1 = rmc::any::<i32>();
    let mut var2 = rmc::any::<i32>();
    let old_var1 = var1;
    unsafe {
        assert_eq!(mem::replace(&mut var1, var2), old_var1);
    }
    assert_eq!(var1, var2);
}
