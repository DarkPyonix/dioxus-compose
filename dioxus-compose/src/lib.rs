#![deny(unsafe_op_in_unsafe_fn)]

pub mod boundary;
#[doc(hidden)]
pub mod codegen;
pub mod drawing;
mod extensions;
pub mod protocol;
pub mod renderer;
pub mod schema;
pub mod tokens;
mod widgets;
pub mod window;

pub use boundary::{
    Host, LaunchBuilder, MutationBatch, RendererApi, install_renderer_api, launch,
    request_frame_from_worker,
};
pub use dioxus_core::{Element, VirtualDom};
pub use dioxus_core_macro::{component, rsx};
pub use drawing::{DrawCommand, DrawList, DrawListBuilder};
pub use elements::*;
pub use extensions::LinearProgressIndicator;
pub use schema::{
    Alignment, Arrangement, AssetKind, ButtonVariant, Color, ColorRole, ColorScheme, DesignSystem,
    EventPayload, IconRole, Key, LoopMode, Modifier, Paint, PropertyKind, SCHEMA_HASH, Selection,
    ShapeRole, SpaceRole, TextAlign, TextOverflow, Theme, TypeRole, WidgetKind, WindowSizeClass,
};
pub use widgets::{
    Button, Canvas, Card, Column, ComposeBox as Box, DatePicker, Dialog, Dropdown, Icon, Image,
    KeyEvent, LazyColumn, LazyRow, Menu, RangeRequest, Row, ScrollColumn, Separator, Spacer,
    Surface, Tabs, Text, TextField, TimePicker, Tooltip, TopAppBar,
};
pub use window::{WindowSize, use_window_size, window_size};

pub mod prelude {
    pub use crate as dioxus_elements;
    // dioxus-core 0.7's rsx! expansion uses unqualified `Box<T>`.
    // Exporting the Compose `Box` through this glob prelude shadows it. Use
    // `dioxus_compose::Box { ... }` in RSX until upstream qualifies std::boxed::Box.
    pub use crate::{
        Alignment, Arrangement, AssetKind, Button, ButtonVariant, Canvas, Card, Color, ColorRole,
        ColorScheme, Column, DatePicker, DesignSystem, Dialog, DrawCommand, DrawList, Dropdown,
        Element, Icon, IconRole, Image, Key, KeyEvent, LaunchBuilder, LazyColumn, LazyRow,
        LinearProgressIndicator, LoopMode, Menu, Modifier, Paint, RangeRequest, Row, ScrollColumn,
        Separator, ShapeRole, SpaceRole, Spacer, Surface, Tabs, Text, TextAlign, TextField,
        TextOverflow, Theme, TimePicker, Tooltip, TopAppBar, TypeRole, WindowSize, WindowSizeClass,
        component, launch, rsx, use_window_size,
    };
    pub use dioxus_core::{Callback, Event, EventHandler, Properties, VirtualDom};
    pub use dioxus_hooks::*;
    pub use dioxus_signals::*;
}

pub mod elements {
    #![allow(non_upper_case_globals)]

    pub type AttributeDescription = (&'static str, Option<&'static str>, bool);

    pub use crate::extensions::elements::*;

    /// The Modifier attributes every widget accepts.
    ///
    /// In Compose a Modifier applies to any composable, and the same has to be true here
    /// or an application cannot give a widget padding, a background, a corner or a border.
    /// Without them a sample can only place bare text and buttons against the window edge,
    /// which is what these samples looked like before this existed.
    ///
    /// They are declared once, on every widget, rather than listed per element, because
    /// the set that applies to a Card and to a Text is the same set.
    macro_rules! modifier_attributes {
        () => {
            pub const weight: AttributeDescription = ("weight", None, false);
            pub const fill_max_width: AttributeDescription = ("fill_max_width", None, false);
            pub const fill_max_height: AttributeDescription = ("fill_max_height", None, false);
            pub const width: AttributeDescription = ("width", None, false);
            pub const height: AttributeDescription = ("height", None, false);
            pub const padding: AttributeDescription = ("padding", None, false);
            pub const padding_role: AttributeDescription = ("padding_role", None, false);
            pub const background: AttributeDescription = ("background", None, false);
            pub const shape_role: AttributeDescription = ("shape_role", None, false);
            pub const corner_radius: AttributeDescription = ("corner_radius", None, false);
            pub const border_width: AttributeDescription = ("border_width", None, false);
            pub const border_color: AttributeDescription = ("border_color", None, false);
            pub const elevation: AttributeDescription = ("elevation", None, false);
            pub const onclickable: AttributeDescription = ("onclickable", None, false);
        };
    }

    macro_rules! element {
        // A widget with no attributes of its own still takes Modifiers, so this arm is the
        // same as the one below with an empty list rather than a smaller module.
        ($module:ident, $tag:literal, []) => {
            pub mod $module {
                use super::AttributeDescription;

                pub const TAG_NAME: &str = $tag;
                pub const NAME_SPACE: Option<&str> = None;
                modifier_attributes!();
            }
        };
        ($module:ident, $tag:literal, [$($attribute:ident),* $(,)?]) => {
            pub mod $module {
                use super::AttributeDescription;

                pub const TAG_NAME: &str = $tag;
                pub const NAME_SPACE: Option<&str> = None;
                $(pub const $attribute: AttributeDescription = (stringify!($attribute), None, false);)*
                modifier_attributes!();
            }
        };
    }

    // Layout containers carry arrangement, spacing and cross-axis alignment.
    element!(
        column,
        "Column",
        [arrangement, spacing, space_role, alignment]
    );
    element!(row, "Row", [arrangement, spacing, space_role, alignment]);
    element!(composebox, "Box", [item_key, alignment]);
    // The type role plus one attribute per override axis, so changing one axis is one
    // SetProp and the rest of the text's styling is not resent.
    element!(
        text,
        "Text",
        [
            text,
            type_role,
            font_size,
            font_weight,
            line_height,
            letter_spacing,
            color,
            text_align,
            max_lines,
            overflow
        ]
    );
    element!(
        textfield,
        "TextField",
        [placeholder, enabled, multiline, type_role]
    );
    element!(button, "Button", [text, enabled, variant, color]);
    // Spacer has no attributes of its own: its size comes from the Modifier attributes
    // every widget carries, which is also how a Compose Spacer is sized.
    element!(spacer, "Spacer", []);
    element!(lazycolumn, "LazyColumn", [item_count]);
    // Widget tags 10 and 11. An asset id is the whole of what they carry: the bytes were
    // copied into the Renderer's cache once, at registration.
    element!(image, "Image", [asset]);
    // An Icon takes a tint as well, through the same Paint attribute Text uses.
    element!(icon, "Icon", [asset, color]);
    // Widget tags 18 to 25. Each one emits roles and children only: how a card, a bar or a
    // popup is drawn belongs to the design system, not to the Host that declared it.
    element!(card, "Card", []);
    element!(surface, "Surface", []);
    // `open` seeds the Renderer's own open state rather than driving it frame by frame: the
    // Renderer owns the enter and exit so neither crosses the boundary.
    element!(dialog, "Dialog", [open]);
    element!(menu, "Menu", [open]);
    // The selection is the Renderer's too. `selected_index` seeds it and moves it when the
    // change came from outside the Renderer.
    element!(tabs, "Tabs", [selected_index]);
    element!(topappbar, "TopAppBar", []);
    element!(lazyrow, "LazyRow", [item_count]);
    element!(tooltip, "Tooltip", [text]);
    // The command list is a byte blob, so it is one attribute and one SetProp. An
    // unchanged list compares equal and produces no mutation at all.
    element!(canvas, "Canvas", [commands]);
    // Widget tags 27 to 29. A picker carries a value, a range and a change event, and
    // nothing that says how the user should pick: a calendar grid, a dial, a wheel or a
    // flyout is the design system's decision.
    element!(datepicker, "DatePicker", [value, min, max, enabled]);
    element!(timepicker, "TimePicker", [value, min, max, enabled]);
    element!(dropdown, "Dropdown", [selected_index, enabled]);
    // Whole content plus a vertical scroll. The position stays in the Renderer.
    element!(scrollcolumn, "ScrollColumn", []);

    #[doc(hidden)]
    pub mod completions {
        #[allow(non_camel_case_types)]
        pub enum CompleteWithBraces {
            column {},
            row {},
            composebox {},
            text {},
            textfield {},
            button {},
            spacer {},
            lazycolumn {},
            scrollcolumn {},
            card {},
            surface {},
            dialog {},
            menu {},
            tabs {},
            topappbar {},
            lazyrow {},
            tooltip {},
            canvas {},
            image {},
            icon {},
            datepicker {},
            timepicker {},
            dropdown {},
        }
    }
}

pub mod events {
    use dioxus_core::{Attribute, Event, ListenerCallback, SpawnIfAsync, SuperInto};

    macro_rules! event {
        ($name:ident, $data:ty) => {
            pub fn $name<Marker>(
                handler: impl SuperInto<ListenerCallback<$data>, Marker>,
            ) -> Attribute {
                Attribute::new(stringify!($name), handler.super_into(), None, false)
            }

            pub mod $name {
                use super::*;

                pub fn call_with_explicit_closure<Marker, Return>(
                    handler: impl FnMut(Event<$data>) -> Return + 'static,
                ) -> Attribute
                where
                    Return: SpawnIfAsync<Marker> + 'static,
                {
                    Attribute::new(
                        stringify!($name),
                        ListenerCallback::new(handler),
                        None,
                        false,
                    )
                }
            }
        };
    }

    event!(onclick, ());
    event!(onvaluechange, String);
    event!(onsubmit, String);
    event!(onfocuslost, ());
    event!(onkeydown, crate::KeyEvent);
    event!(onrangerequest, crate::RangeRequest);
    // A dismissal carries no value, so it reuses the empty event payload a click uses.
    event!(ondismiss, ());
    // A picker reports the value the user landed on, as the epoch integer the widget
    // speaks. It shares the wire property with the text field's value change, because
    // both are "this control's value is now this".
    event!(onchange, i64);
}
