//! Sample entrypoint: wires the sample modules together.

mod model;
mod util;

fn main() {
    let person = model::Person::new("Ada");
    println!("{}", util::greet(&person.name));
    util::helper();
}
