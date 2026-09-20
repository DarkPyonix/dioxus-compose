package dioxus.compose

/**
 * The vocabulary a caller speaks to a design system.
 *
 * A caller names a role. The design system decides what it looks like. That direction is
 * the whole point: it is what lets a fourth design system be added without touching a
 * single widget, and what stops Cupertino and Fluent from becoming skins over Material.
 *
 * These deliberately mirror the renderer's protocol enums without importing them. Two
 * declarations of the same vocabulary is the price of this project being publishable on
 * its own, and the renderer owns the mapping between them.
 */
enum class ColorRole {
    Primary, OnPrimary,
    Secondary, OnSecondary,
    Surface, OnSurface,
    SurfaceVariant, OnSurfaceVariant,
    Background, OnBackground,
    Outline, OutlineVariant,
    Error, OnError,
}

enum class TypeRole {
    Display, Headline, Title, Subtitle,
    Body, BodyStrong, Label, Caption, Mono,
}

enum class ShapeRole { None, ExtraSmall, Small, Medium, Large, Full }

enum class SpaceRole { None, Xs, Sm, Md, Lg, Xl, Xxl }

enum class ButtonVariant { Filled, Tonal, Outlined, Text }

/**
 * Which design system an implementation is.
 *
 * Six, because that is how many the adaptive theme can pick: one per platform look it
 * follows, plus the Linux fallback.
 */
enum class DesignSystemId {
    Material3,
    Cupertino,
    Fluent,
    Gnome,
    Breeze,
    Deepin,
}

/**
 * The small interactive controls, which differ by more than colour between systems.
 *
 * A Material checkbox is a filled square that draws a tick, a Cupertino one is a circle,
 * and a Fluent one is a square with a lighter top edge. None of that belongs in a widget,
 * so the widget names the kind and the design system answers with the rest.
 */
enum class ControlKind { Checkbox, RadioButton, Switch, Slider }

/**
 * How a value is chosen, which is the part of a picker that is not a matter of styling.
 *
 * The three systems are not the same control painted differently: a Material date is
 * picked from a calendar grid or typed, a Cupertino one is scrolled on a wheel, a Fluent
 * one drops a calendar flyout. A design system answers with the arrangement it uses and
 * the renderer implements the closed set, so a widget never asks for a wheel.
 */
enum class PickerPresentation { CalendarGrid, Wheel, Flyout, Dial, Menu, ComboBox }

/** Which picker is being presented. */
enum class PickerKind { Date, Time, Choice }

/** The things that appear over the rest of the screen rather than in it. */
enum class OverlayKind { Dialog, Menu, Tooltip }

/**
 * How an overlay arrives and where it sits.
 *
 * A dialog is centred with a scrim on Material and on Fluent, and rises as a sheet on
 * iOS. A menu is a dropdown, a popover or a flyout depending on who is asking.
 */
enum class OverlayPresentation { CenteredModal, Sheet, Popover, Flyout, Dropdown }

/** The navigation surfaces, which differ in shape and in where the title sits. */
enum class NavigationKind { Tabs, TopAppBar }

/** How a tab strip marks the selected tab. */
enum class TabIndicator { Underline, Segmented, Pivot }

/** Where a bar puts its title. Material puts it at the start, Cupertino centres it. */
enum class TitleAlignment { Start, Center }

/** What a list does when it is dragged past its end. */
enum class OverscrollBehaviour { Stretch, RubberBand, None }

/** A closed set of icon meanings, so no system icon name ever crosses the boundary. */
enum class IconRole {
    Back, Forward, Close, Search, Add, Remove, Delete, Edit, Share, More,
    Settings, Check, Warning, Error, Info, Refresh, Copy, Paste, Menu, Send,
}
