// compile-flags: --edition 2015
#![allow(unused)]
#![feature(global_asm, asm_const)]
fn main() {
#[cfg(any(target_arch="x86", target_arch="x86_64"))]
mod x86 {
global_asm!("movl ${}, %ecx", const 5, options(att_syntax));
// is equivalent to
global_asm!("mov ecx, {}", const 5);
}
}