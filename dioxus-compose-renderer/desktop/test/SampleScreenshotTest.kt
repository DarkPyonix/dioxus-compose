package dioxus.compose.test

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asSkiaBitmap
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.runComposeUiTest
import androidx.compose.ui.unit.Density
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
 * Draws a real sample's frames and writes a PNG beside the frames they came from.
 *
 * The showcase next door is a script of primitives written to exercise the token tables.
 * A sample is an application laid out the way someone would really lay one out, and that
 * is where a token turns out to be wrong in a way no ladder of swatches shows: a readout
 * whose fill matches the page behind it is drawn full size, in the right colour, and
 * cannot be seen. Six systems that nobody has looked at under a real screen is six
 * chances at that.
 *
 * The frames are the bytes the Host encoded, written by the samples' own `DXC_FRAME_DIR`
 * runs, so what gets drawn here is what an application would send and not a Kotlin
 * restatement of it. A screen can take more than one batch to build, a list holds no rows
 * until something asks for a window, so a file is a sequence of batches, each behind four
 * little endian bytes of its length.
 *
 * **The window is sized from the file's own name.** A screen that adapts is two claims,
 * and they are made on opposite sides of the boundary: the Host chose what to put on the
 * screen from the size class it was told about, and the Renderer chooses a bar, a rail or
 * a drawer from the width it measures here. A picture taken at one width can only ever
 * show one of the two, and a picture of a wide layout squeezed into a phone window shows
 * neither. So a name ending in one of the class suffixes below is drawn in a window of
 * that class, and the frames it holds are the ones the Host produced after being told the
 * same thing.
 *
 * Enabled only when `DXC_FRAME_DIR` names a directory holding those frames, because it
 * reads and writes files.
 */
@OptIn(ExperimentalTestApi::class)
class SampleScreenshotTest {
    @Test
    fun fr20_sample_screenshots() {
        val directory = System.getenv("DXC_FRAME_DIR")?.let(::File) ?: return
        val frames = directory.listFiles { file -> file.extension == "bin" }?.sorted().orEmpty()
        check(frames.isNotEmpty()) {
            "${directory.absolutePath} holds no frames. Record them first with " +
                "`DXC_FRAME_DIR=${directory.absolutePath} cargo test -p sample-calculator " +
                "fr14_the_first_frame` and the same for the other samples."
        }
        frames.forEach { frame ->
            val mutations = mutableListOf<Mutation>()
            val file = ByteBuffer.wrap(frame.readBytes()).order(ByteOrder.LITTLE_ENDIAN)
            while (file.remaining() >= Int.SIZE_BYTES) {
                val length = file.int
                check(length in 0..file.remaining()) {
                    "${frame.name} claims a batch of $length bytes with ${file.remaining()} left"
                }
                val batch = file.slice().order(ByteOrder.LITTLE_ENDIAN).limit(length)
                Protocol.decode(batch, mutations::add)
                file.position(file.position() + length)
            }
            check(mutations.isNotEmpty()) { "${frame.name} decoded to no records" }
            val scene = sceneFor(frame.nameWithoutExtension)
            runComposeUiTest {
                setContent {
                    // A fixed density, so a picture taken on a Retina machine and one
                    // taken on a build server are the same picture. Without it the dp
                    // sizes below come out as different numbers of pixels and the two
                    // cannot be compared.
                    CompositionLocalProvider(LocalDensity provides Density(1f)) {
                        Box(Modifier.size(scene.widthDp.dp, scene.heightDp.dp)) {
                            DioxusContent(rememberDioxusHost(FakeHostConnection(mutations)))
                        }
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

    /** The window one frame file is drawn in. */
    private data class Scene(val widthDp: Int, val heightDp: Int)

    /**
     * The window a frame file asks for, read off the end of its name.
     *
     * The widths sit well inside their classes rather than on the boundaries, because a
     * picture is meant to show what that class ordinarily looks like and a window one dp
     * over a boundary shows the narrowest version of it. A name that says nothing gets a
     * phone, which is what every frame recorded before this existed was drawn in.
     */
    private fun sceneFor(name: String): Scene = when (name.substringAfterLast('-')) {
        "compact" -> Scene(420, 760)
        "medium" -> Scene(720, 760)
        "expanded" -> Scene(1180, 800)
        else -> Scene(420, 760)
    }
}
