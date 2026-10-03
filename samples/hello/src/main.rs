//! The desktop program. The page is in the library beside this, where the tests reach it
//! too.

// A window application rather than a console one: without this, double clicking the
// executable on Windows opens a terminal beside the window. Run from a terminal, the
// renderer still writes there.
#![windows_subsystem = "windows"]

fn main() {
    sample_html_hello::launch();
}
