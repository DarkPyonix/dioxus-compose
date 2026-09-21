package dioxus.compose.test

import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.isRoot
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.runDesktopComposeUiTest
import java.io.File
import kotlin.test.Test
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image
import dioxus.compose.protocol.ColorScheme
import dioxus.compose.protocol.DesignSystem
import dioxus.compose.protocol.Theme
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.designShowcaseHost
import dioxus.compose.tooling.overlayShowcaseHost

/**
 * The scene the showcase is rendered into, tall enough for the whole of it.
 *
 * The point of these images is comparing the systems by eye, and a window that cuts the
 * bottom off hides exactly the controls that were added last.
 */
private const val SHOWCASE_WIDTH = 520
private const val SHOWCASE_HEIGHT = 2100

/** Room for a dialog to sit centred with the page showing around it. */
private const val OVERLAY_WIDTH = 520
private const val OVERLAY_HEIGHT = 420

/**
 * Renders the design showcase for each system and writes a PNG, so the systems can be
 * compared by eye. Whether they really look different is not something an assertion
 * settles. Enabled only when `DXC_SCREENSHOT_DIR` is set, because it writes files.
 */
@OptIn(ExperimentalTestApi::class)
class DesignScreenshotTest {
    @Test
    fun fr14_showcase_screenshots() {
        val directory = System.getenv("DXC_SCREENSHOT_DIR") ?: return
        File(directory).mkdirs()
        DesignSystem.entries.forEach { system ->
            listOf(ColorScheme.Light, ColorScheme.Dark).forEach { scheme ->
                // The scene's own size is what bounds the capture, so a Box cannot make
                // room: anything past the bottom of the window is simply not in the image.
                runDesktopComposeUiTest(SHOWCASE_WIDTH, SHOWCASE_HEIGHT) {
                    setContent {
                        DioxusContent(
                            rememberDioxusHost(
                                designShowcaseHost(Theme(system, system, scheme, false)),
                            ),
                        )
                    }
                    val bitmap = onRoot().captureToImage().asSkiaBitmap()
                    val data = Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)
                        ?: error("PNG encoding failed")
                    File(directory, "${system.name}-${scheme.name}.png").writeBytes(data.bytes)
                }
            }
        }
    }

    /**
     * The same for the two things that are drawn over a window rather than in it.
     *
     * A dialog and a menu were the last surfaces here that had never been on screen under
     * any system, because putting either one in the showcase next door covers the whole of
     * it. They get their own scene, one per overlay per system.
     */
    @Test
    fun fr14_overlay_screenshots() {
        val directory = System.getenv("DXC_SCREENSHOT_DIR") ?: return
        File(directory).mkdirs()
        DesignSystem.entries.forEach { system ->
            listOf("dialog" to true, "menu" to false).forEach { (name, dialog) ->
                runDesktopComposeUiTest(OVERLAY_WIDTH, OVERLAY_HEIGHT) {
                    setContent {
                        DioxusContent(
                            rememberDioxusHost(
                                overlayShowcaseHost(
                                    Theme(system, system, ColorScheme.Light, false),
                                    dialog,
                                ),
                            ),
                        )
                    }
                    waitForIdle()
                    // An overlay is its own root: the page is one and the popup laid over
                    // it is another, and the popup is the one with the thing in it.
                    val roots = onAllNodes(isRoot())
                    val bitmap = roots[roots.fetchSemanticsNodes().size - 1]
                        .captureToImage()
                        .asSkiaBitmap()
                    val data = Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)
                        ?: error("PNG encoding failed")
                    File(directory, "overlay-${system.name}-$name.png").writeBytes(data.bytes)
                }
            }
        }
    }
}
