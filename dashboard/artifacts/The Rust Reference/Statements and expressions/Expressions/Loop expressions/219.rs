// compile-flags: --edition 2021
#![allow(unused)]
pub fn main() {
'outer: loop {
    while true {
        break 'outer;
    }
}
}