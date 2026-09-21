package dioxus.compose.tooling

import androidx.compose.runtime.remember
import androidx.compose.ui.window.Window
import androidx.compose.ui.window.application
import dioxus.compose.protocol.ButtonVariant
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.Paint
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue
import dioxus.compose.protocol.ShapeRole
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.protocol.Theme
import dioxus.compose.protocol.TypeRole
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import java.lang.System

/**
 * A scripted Host that exercises the design primitives under one design system, so the
 * result can be looked at rather than only asserted on.
 *
 * Nothing here is design system aware: the same records produce Material 3, Cupertino or
 * Fluent pixels depending only on the `SetTheme` at the front.
 */
fun designShowcaseHost(theme: Theme): FakeHostConnection {
    val records = mutableListOf<Mutation>(Mutation.SetTheme(theme))
    var nextId = 1
    fun id(): Int = nextId++

    fun text(
        parent: Int,
        index: Int,
        value: String,
        role: TypeRole,
        color: ColorRole = ColorRole.OnSurface,
    ) {
        val node = id()
        records += Mutation.Create(node, WidgetKind.Text)
        records += Mutation.SetProp(node, PropertyKind.Text, PropertyValue.Text(value))
        records += Mutation.SetProp(node, PropertyKind.TypeRole, PropertyValue.Integer(role.ordinal + 1L))
        records += Mutation.SetProp(node, PropertyKind.Color, PropertyValue.Integer(roleBits(color)))
        records += Mutation.Insert(parent, node, index)
    }

    val root = id()
    records += Mutation.Create(root, WidgetKind.ScrollColumn)
    records += Mutation.SetModifier(root, 0, ProtocolModifier.FillMaxWidth)
    records += Mutation.SetModifier(root, 1, ProtocolModifier.FillMaxHeight)
    records += Mutation.SetModifier(root, 2, ProtocolModifier.Background(Paint.Role(ColorRole.Surface)))
    records += Mutation.SetModifier(root, 3, ProtocolModifier.PaddingRole(SpaceRole.Lg))
    records += Mutation.SetProp(root, PropertyKind.SpaceRole, PropertyValue.Integer(SpaceRole.Md.ordinal + 1L))

    var slot = 0
    text(root, slot++, systemName(theme), TypeRole.Display)
    text(root, slot++, "the nine rung type ladder", TypeRole.Headline, ColorRole.OnSurfaceVariant)
    listOf(
        TypeRole.Title to "Title sets a section",
        TypeRole.Subtitle to "Subtitle carries it",
        TypeRole.Body to "Body is the reading size for long copy.",
        TypeRole.BodyStrong to "BodyStrong is the same size with more weight.",
        TypeRole.Label to "LABEL",
        TypeRole.Caption to "Caption is the smallest rung",
        TypeRole.Mono to "mono 0O1lI",
    ).forEach { (role, sample) -> text(root, slot++, sample, role) }

    // The four button variants, side by side.
    val buttons = id()
    records += Mutation.Create(buttons, WidgetKind.Row)
    records += Mutation.SetProp(buttons, PropertyKind.SpaceRole, PropertyValue.Integer(SpaceRole.Sm.ordinal + 1L))
    records += Mutation.Insert(root, buttons, slot++)
    ButtonVariant.entries.forEachIndexed { index, variant ->
        val node = id()
        records += Mutation.Create(node, WidgetKind.Button)
        records += Mutation.SetProp(node, PropertyKind.Text, PropertyValue.Text(variant.name))
        records += Mutation.SetProp(node, PropertyKind.Variant, PropertyValue.Integer(variant.ordinal + 1L))
        records += Mutation.SetProp(node, PropertyKind.OnClick, PropertyValue.Integer(1L))
        records += Mutation.Insert(buttons, node, index)
    }

    // An elevated card: the same dp is a tonal rise, a soft shadow or a layer stroke.
    val card = id()
    records += Mutation.Create(card, WidgetKind.Box)
    records += Mutation.SetModifier(card, 0, ProtocolModifier.FillMaxWidth)
    records += Mutation.SetModifier(card, 1, ProtocolModifier.ShapeRole(ShapeRole.Large))
    records += Mutation.SetModifier(card, 2, ProtocolModifier.Elevation(6f))
    records += Mutation.SetModifier(card, 3, ProtocolModifier.Background(Paint.Role(ColorRole.SurfaceVariant)))
    records += Mutation.SetModifier(card, 4, ProtocolModifier.PaddingRole(SpaceRole.Lg))
    records += Mutation.Insert(root, card, slot++)
    text(card, 0, "Elevation 6 dp, ShapeRole.Large", TypeRole.BodyStrong, ColorRole.OnSurfaceVariant)

    // Weight: 1 to 3 across the row, which only the parent scope can express.
    val weights = id()
    records += Mutation.Create(weights, WidgetKind.Row)
    records += Mutation.SetModifier(weights, 0, ProtocolModifier.FillMaxWidth)
    records += Mutation.SetModifier(weights, 1, ProtocolModifier.Height(48f))
    records += Mutation.SetProp(weights, PropertyKind.SpaceRole, PropertyValue.Integer(SpaceRole.Sm.ordinal + 1L))
    records += Mutation.Insert(root, weights, slot++)
    listOf(1f to ColorRole.Primary, 3f to ColorRole.Secondary).forEachIndexed { index, (weight, color) ->
        val node = id()
        records += Mutation.Create(node, WidgetKind.Box)
        records += Mutation.SetModifier(node, 0, ProtocolModifier.Weight(weight))
        records += Mutation.SetModifier(node, 1, ProtocolModifier.FillMaxHeight)
        records += Mutation.SetModifier(node, 2, ProtocolModifier.ShapeRole(ShapeRole.Small))
        records += Mutation.SetModifier(node, 3, ProtocolModifier.Background(Paint.Role(color)))
        records += Mutation.Insert(weights, node, index)
    }

    // A bordered shape role, and a field, so text input is visible in the same window.
    val field = id()
    records += Mutation.Create(field, WidgetKind.TextField)
    records += Mutation.SetProp(field, PropertyKind.Placeholder, PropertyValue.Text("type here"))
    records += Mutation.SetModifier(field, 0, ProtocolModifier.FillMaxWidth)
    records += Mutation.SetModifier(field, 1, ProtocolModifier.ShapeRole(ShapeRole.Small))
    records += Mutation.SetModifier(field, 2, ProtocolModifier.Border(1f, Paint.Role(ColorRole.Outline)))
    records += Mutation.SetModifier(field, 3, ProtocolModifier.PaddingRole(SpaceRole.Sm))
    records += Mutation.Insert(root, field, slot++)

    val connection = FakeHostConnection(records)
    connection.respondWith { HostResponse(result = 1) }
    return connection
}

private fun roleBits(role: ColorRole): Long = (1L shl 32) or (role.ordinal + 1L)

private fun systemName(theme: Theme): String = when (theme.designSystem) {
    DesignSystem.Material3 -> "Material 3"
    DesignSystem.Cupertino -> "Cupertino"
    DesignSystem.Fluent -> "WinUI Fluent 2"
    DesignSystem.Gnome -> "GNOME 50 Adwaita"
}

/**
 * Opens the showcase for one design system.
 *
 * `DXC_DESIGN_SYSTEM` is `material3`, `hig`, `fluent` or `gnome`, `DXC_COLOR_SCHEME` is `light`,
 * `dark` or `system`, and `DXC_ADAPTIVE=1` makes the Host send an adaptive theme with the
 * chosen system as the mandatory fallback.
 */
fun main() = application {
    val system = when (System.getenv("DXC_DESIGN_SYSTEM")?.lowercase()) {
        "hig", "apple", "applehig" -> DesignSystem.Cupertino
        "fluent", "winui" -> DesignSystem.Fluent
        "gnome", "adwaita" -> DesignSystem.Gnome
        else -> DesignSystem.Material3
    }
    val scheme = when (System.getenv("DXC_COLOR_SCHEME")?.lowercase()) {
        "dark" -> ColorScheme.Dark
        "system", "follow" -> ColorScheme.FollowSystem
        else -> ColorScheme.Light
    }
    val adaptive = System.getenv("DXC_ADAPTIVE") == "1"
    val theme = Theme(system, system, scheme, adaptive)
    Window(onCloseRequest = ::exitApplication, title = "dioxus-compose design showcase") {
        val connection = remember { designShowcaseHost(theme) }
        DioxusContent(rememberDioxusHost(connection))
    }
}
