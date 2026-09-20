package dioxus.compose

/**
 * The vocabulary a caller speaks to a design system (SPEC FR-13, FR-14).
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
