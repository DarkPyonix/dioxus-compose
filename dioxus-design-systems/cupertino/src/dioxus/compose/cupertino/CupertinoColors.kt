package dioxus.compose.cupertino

import androidx.compose.runtime.Immutable
import androidx.compose.ui.graphics.Color
import dioxus.compose.ColorRole

/**
 * Apple's system colours, in the two schemes.
 *
 * These are not one palette dimmed for dark mode. Apple publishes a separate value per
 * scheme for every system colour, and several of them move in ways a lightness transform
 * would not produce: systemBlue gets brighter in the dark rather than darker, and the
 * grouped backgrounds invert their order entirely, with the grouped surface sitting
 * lighter than the page in the dark and darker than it in the light. Deriving one scheme
 * from the other would lose that, so both are written out.
 *
 * Values are the sRGB ones Apple documents for iOS and macOS system colours.
 */
@Immutable
internal class CupertinoPalette(
    val blue: Color,
    val red: Color,
    val label: Color,
    val secondaryLabel: Color,
    val separator: Color,
    val separatorSoft: Color,
    /** The page behind everything. */
    val background: Color,
    /** A grouped content surface sitting on the page. */
    val surface: Color,
    /** A recessed or filled area inside a surface. */
    val surfaceVariant: Color,
    /** The fill behind a grey control. */
    val controlFill: Color,
) {
    fun resolve(role: ColorRole): Color = when (role) {
        ColorRole.Primary -> blue
        ColorRole.OnPrimary -> Color.White
        ColorRole.Secondary -> controlFill
        // A grey Apple control carries a tinted label, not a black one, which is what
        // tells the reader it is a control and not a panel.
        ColorRole.OnSecondary -> blue
        ColorRole.Surface -> surface
        ColorRole.OnSurface -> label
        ColorRole.SurfaceVariant -> surfaceVariant
        ColorRole.OnSurfaceVariant -> secondaryLabel
        ColorRole.Background -> background
        ColorRole.OnBackground -> label
        ColorRole.Outline -> separator
        ColorRole.OutlineVariant -> separatorSoft
        ColorRole.Error -> red
        ColorRole.OnError -> Color.White
    }

    companion object {
        val Light = CupertinoPalette(
            blue = Color(0xFF007AFF),
            red = Color(0xFFFF3B30),
            label = Color(0xFF000000),
            // Apple's secondaryLabel is black at 60 percent. Flattened here against the
            // surface it is read on, because a role has to answer with one colour and a
            // translucent answer would make its contrast depend on the caller.
            secondaryLabel = Color(0xFF5B5B60),
            separator = Color(0xFFC6C6C8),
            separatorSoft = Color(0xFFE5E5EA),
            background = Color(0xFFFFFFFF),
            surface = Color(0xFFF2F2F7),
            surfaceVariant = Color(0xFFE5E5EA),
            controlFill = Color(0xFFE5E5EA),
        )

        val Dark = CupertinoPalette(
            blue = Color(0xFF0A84FF),
            red = Color(0xFFFF453A),
            label = Color(0xFFFFFFFF),
            secondaryLabel = Color(0xFFAEAEB2),
            separator = Color(0xFF38383A),
            separatorSoft = Color(0xFF2C2C2E),
            background = Color(0xFF000000),
            surface = Color(0xFF1C1C1E),
            surfaceVariant = Color(0xFF2C2C2E),
            controlFill = Color(0xFF2C2C2E),
        )
    }
}
