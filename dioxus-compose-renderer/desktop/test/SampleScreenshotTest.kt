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
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.Protocol
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.test.Test
import org.jetbrains.skia.EncodedImageFormat
import org.jetbrains.skia.Image

/**
 * Draws a real sample's first frame, once per design system, and writes a PNG beside the
 * frame it came from.
 *
 * The showcase next door is a script of primitives written to exercise the token tables.
 * A sample is an application laid out the way someone would really lay one out, and that
 * is where a token turns out to be wrong in a way no ladder of swatches shows: a readout
 * whose fill matches the page behind it is drawn full size, in the right colour, and
 * cannot be seen. Six systems that nobody has looked at under a real screen is six
 * chances at that.
 *
 * The frames are the bytes the Host encoded, written by the calculator's
 * `DXC_FRAME_DIR` run, so what gets drawn here is what an application would send and not
 * a Kotlin restatement of it. Enabled only when `DXC_FRAME_DIR` names a directory holding
 * those frames, because it reads and writes files.
 */
@OptIn(ExperimentalTestApi::class)
class SampleScreenshotTest {
    @Test
    fun fr14_sample_screenshots() {
        val directory = System.getenv("DXC_FRAME_DIR")?.let(::File) ?: return
        val frames = directory.listFiles { file -> file.extension == "bin" }?.sorted().orEmpty()
        check(frames.isNotEmpty()) {
            "${directory.absolutePath} holds no frames. Record them first with " +
                "`DXC_FRAME_DIR=${directory.absolutePath} cargo test -p sample-calculator " +
                "fr14_the_first_frame`."
        }
        frames.forEach { frame ->
            val mutations = mutableListOf<Mutation>()
            Protocol.decode(
                ByteBuffer.wrap(frame.readBytes()).order(ByteOrder.LITTLE_ENDIAN),
                mutations::add,
            )
            check(mutations.isNotEmpty()) { "${frame.name} decoded to no records" }
            runComposeUiTest {
                setContent {
                    // A phone sized window. The calculator lays its keypad out differently
                    // once the window is wide, and one shape per picture keeps the six
                    // comparable.
                    Box(Modifier.size(420.dp, 760.dp)) {
                        DioxusContent(rememberDioxusHost(FakeHostConnection(mutations)))
                    }
                }
                waitForIdle()
                val bitmap = onRoot().captureToImage().asSkiaBitmap()
                val data = Image.makeFromBitmap(bitmap).encodeToData(EncodedImageFormat.PNG)
                    ?: error("${frame.name} could not be encoded as a PNG")
                File(directory, "${frame.nameWithoutExtension}.png").writeBytes(data.bytes)
            }
        }
    }
}
