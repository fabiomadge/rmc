// compile-flags: --edition 2021
#![allow(unused)]
#![feature(inline_const)]

pub fn main() {
const fn one() -> i32 { 1 }

let some_int = 3;
match some_int {
    const { 1 + 2 } => println!("Matched 1 + 2"),
    const { one() } => println!("Matched const fn returning 1"),
    _ => println!("Didn't match anything :("),
}
}