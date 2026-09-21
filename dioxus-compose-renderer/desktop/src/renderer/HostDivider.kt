package dioxus.compose.foundation

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import dioxus.compose.design.ResolvedTheme
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.ui.node.Node

/**
 * A Divider drawn by the active design system's rules.
 *
 * The axis is the only thing the Host decides. The thickness, the colour and how far the
 * rule is held back from the leading edge come from
 * [dioxus.compose.design.ComponentRules.controls]: a Material rule runs the full width, a
 * Cupertino one is a hairline inset the way a grouped list rules between its rows.
 */
@Composable
internal fun HostDivider(node: Node, modifier: Modifier, theme: ResolvedTheme) {
    val style = theme.rules.controls(theme).divider
    val vertical = node.flag(PropertyKind.Vertical, default = false)
    if (vertical) {
        Box(
            modifier
                .padding(top = style.inset)
                .width(style.thickness)
                .fillMaxHeight()
                .background(style.color),
        )
    } else {
        Box(
            modifier
                .padding(start = style.inset)
                .height(style.thickness)
                .fillMaxWidth()
                .background(style.color),
        )
    }
}
