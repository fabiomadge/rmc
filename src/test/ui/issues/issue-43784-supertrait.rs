// Copyright 2017 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

pub trait Partial: Copy {
}

pub trait Complete: Partial {
}

impl<T> Partial for T where T: Complete {}
impl<T> Complete for T {} //~ ERROR the trait bound `T: std::marker::Copy` is not satisfied

fn main() {}
