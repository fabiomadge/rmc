// compile-flags: --edition 2021
#![allow(unused)]
#![feature(global_asm, asm_const)]
pub fn main() {
#[cfg(any(target_arch="x86", target_arch="x86_64"))]
mod x86 {
const C: i32 = 1234;
global_asm!(
    ".global bar",
    "bar: .word {c}",
    c = const C,
);
}
}