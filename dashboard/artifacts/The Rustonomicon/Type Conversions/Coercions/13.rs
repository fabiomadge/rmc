// rmc-check-fail
// compile-flags: --edition 2021
#![allow(unused)]
trait Trait {}

fn foo<X: Trait>(t: X) {}

impl<'a> Trait for &'a i32 {}

pub fn main() {
    let t: &mut i32 = &mut 0;
    foo(t);
}