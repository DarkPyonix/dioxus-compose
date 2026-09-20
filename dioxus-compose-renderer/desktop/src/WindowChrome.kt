package dioxus.compose.ui.platform

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.ui.unit.dp
import java.awt.Window as AwtWindow
import javax.swing.JRootPane
import javax.swing.RootPaneContainer

/**
 * Modern window decoration, which is what every desktop platform now expects.
 *
 * macOS moved the title bar into the application's own content years ago, GNOME draws
 * client side decorations, and Windows 11 applications generally paint their own caption.
 * A window with a separate system title bar looks a decade old on all three.
 *
 * The platforms want genuinely different things, so this does not invent a common
 * abstraction over them:
 *
 * - macOS keeps its real title bar but makes it transparent and lets content run
 *   underneath. The close, minimise and zoom buttons stay the system's. Drawing those
 *   ourselves would be the most visible way to fail at looking native: their placement,
 *   hover behaviour, full screen transition and accessibility labels all belong to the
 *   system, and an imitation drifts from the real thing on the next OS release.
 * - Windows and Linux get an undecorated window and we paint the caption, because neither
 *   offers an equivalent of the macOS client properties through AWT.
 */
internal enum class WindowChrome {
    /** Content extends into the title bar area. The default. */
    Modern,

    /** The platform's ordinary title bar. */
    System,
}

/**
 * True when this platform keeps its system window buttons under [WindowChrome.Modern].
 *
 * Only macOS does. Everywhere else the caption is ours to draw, so the renderer has to
 * supply buttons as well as insets.
 */
internal val platformDrawsWindowButtons: Boolean
    get() = System.getProperty("os.name").orEmpty().startsWith("Mac")

/**
 * Applies [chrome] to an already created window.
 *
 * On macOS this is three AWT client properties and no native code:
 * `fullWindowContent` extends the content view under the title bar, `transparentTitleBar`
 * stops the bar painting its own background, and `windowTitleVisible` hides the text,
 * which would otherwise float over application content. The JDK has supported these since
 * 17, so no JNI and no reachability metadata are involved.
 *
 * Called after the window exists because AWT reads these when the peer is realised.
 */
internal fun applyWindowChrome(window: AwtWindow, chrome: WindowChrome) {
    if (chrome == WindowChrome.System) return
    val root: JRootPane = (window as? RootPaneContainer)?.rootPane ?: return
    if (platformDrawsWindowButtons) {
        root.putClientProperty("apple.awt.fullWindowContent", true)
        root.putClientProperty("apple.awt.transparentTitleBar", true)
        root.putClientProperty("apple.awt.windowTitleVisible", false)
    }
}

/**
 * How far content must be inset to clear the window buttons and the draggable caption.
 *
 * Content is allowed to run underneath the caption, which is the point, but a widget
 * placed where the macOS traffic lights are would leave both unusable. The renderer
 * applies this to the root node; the Host never sees it and cannot set it, because the
 * safe area is a fact about the window rather than a decision the application makes.
 *
 * 28dp is the height of the macOS title bar area, and 78dp clears the three buttons plus
 * the gap after them.
 */
internal fun windowContentInsets(chrome: WindowChrome, hasTopAppBar: Boolean): PaddingValues =
    when {
        chrome == WindowChrome.System -> PaddingValues(0.dp)
        // A TopAppBar is the caption, so it positions itself around the buttons rather
        // than being pushed below them.
        hasTopAppBar -> PaddingValues(0.dp)
        platformDrawsWindowButtons -> PaddingValues(top = 28.dp)
        // Where we draw the caption ourselves, its height is ours to choose.
        else -> PaddingValues(top = 32.dp)
    }

/** The horizontal room the system window buttons occupy, for a caption to lay out around. */
internal val systemWindowButtonsWidth
    get() = if (platformDrawsWindowButtons) 78.dp else 0.dp
