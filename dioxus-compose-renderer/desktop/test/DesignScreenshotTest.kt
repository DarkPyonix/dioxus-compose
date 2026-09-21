package dioxus.compose.test

import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
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

/**
 * The scene the showcase is rendered into, tall enough for the whole of it.
 *
 * The point of these images is comparing the systems by eye, and a window that cuts the
 * bottom off hides exactly the controls that were added last.
 */
private const val SHOWCASE_WIDTH = 520
private const val SHOWCASE_HEIGHT = 1700

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
}
