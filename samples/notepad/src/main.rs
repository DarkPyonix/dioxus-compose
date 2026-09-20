use dioxus_compose::prelude::*;

fn app() -> Element {
    rsx! {
        Column {
            Text { text: "sample-notepad: not implemented yet" }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
