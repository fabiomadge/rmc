// compile-flags: --edition 2021
#![allow(unused)]
extern {
    fn foo(x: i32, ...);
}

pub fn main() {
    unsafe {
        foo(10, 20, 30, 40, 50);
    }
}