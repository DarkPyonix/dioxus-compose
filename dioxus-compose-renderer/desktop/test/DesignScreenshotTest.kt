package dioxus.compose.test

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.dp
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
 * Renders the design showcase for each system and writes a PNG, so the three systems can
 * be compared by eye. Whether they really look different is not something an assertion
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
                runComposeUiTest {
                    setContent {
                        Box(Modifier.size(520.dp, 900.dp)) {
                            DioxusContent(
                                rememberDioxusHost(
                                    designShowcaseHost(Theme(system, system, scheme, false)),
                                ),
                            )
                        }
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
