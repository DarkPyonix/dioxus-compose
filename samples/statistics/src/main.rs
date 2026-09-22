//! The desktop program. Everything it draws is in the library beside this, because the
//! same screens have to be reachable from an Android Activity and from a browser page,
//! and neither of those has a `main` to put them in.

fn main() {
    sample_statistics::launch();
}
