use dioxus_compose::prelude::*;

fn app() -> Element {
    rsx! {
        Column {
            Text { text: "sample-calculator: not implemented yet" }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
