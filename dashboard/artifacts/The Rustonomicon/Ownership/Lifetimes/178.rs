// rmc-check-fail
// compile-flags: --edition 2018
#![allow(unused)]
pub fn main() {
let mut data = vec![1, 2, 3];
let x = &data[0];
data.push(4);
println!("{}", x);
}