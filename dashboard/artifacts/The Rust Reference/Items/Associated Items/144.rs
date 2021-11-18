// compile-flags: --edition 2021
#![allow(unused)]
pub fn main() {
trait Changer: Sized {
    fn change(mut self) {}
    fn modify(mut self: Box<Self>) {}
}
}