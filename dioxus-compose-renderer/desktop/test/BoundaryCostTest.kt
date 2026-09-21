package dioxus.compose.test

import java.lang.management.ManagementFactory
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.charset.StandardCharsets
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.Protocol
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.protocol.PropertyValue

/**
 * What the Renderer spends to read one changed string out of the Host's arena.
 *
 * The arena is the Host's memory, read where it lies, so the only thing this side should
 * have to build is the string Compose is given. These measurements say how close that is
 * to true, and they are the only cheap way to ask: allocation is not visible in the
 * interpreter's output, so without a number here a copy can be added and nothing notices.
 *
 * Measured on the JVM development shell, which is the same bytecode the native image is
 * compiled from. Figures are bytes allocated by this thread, averaged over a run long
 * enough that the measurement's own cost disappears.
 */
class BoundaryCostTest {
    private val threads =
        ManagementFactory.getThreadMXBean() as com.sun.management.ThreadMXBean

    private var sink: Mutation? = null
    private val collect: (Mutation) -> Unit = { sink = it }

    /**
     * One `SetProp(node, Text, text)` record, with `trailing` unused bytes after the
     * string so the same record can be read out of arenas of different sizes.
     */
    private fun textChange(text: String, trailing: Int = 0): ByteBuffer {
        val utf8 = text.toByteArray(StandardCharsets.UTF_8)
        val records = ENVELOPE_BYTES + SET_PROP_BYTES
        val bytes = ByteArray(records + utf8.size + trailing)
        val out = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        out.putShort(0)
        out.putShort(ENVELOPE_BYTES.toShort())
        out.putInt(records)
        out.putInt(1)
        out.putShort(2)
        out.putShort(SET_PROP_BYTES.toShort())
        out.putInt(7)
        out.putShort(1)
        out.putShort(1)
        out.putInt(records)
        out.putInt(utf8.size)
        out.putInt(0)
        out.put(utf8)
        return ByteBuffer.wrap(bytes)
    }

    /** Bytes this thread allocates for one decode of `batch`, once everything is warm. */
    private fun bytesPerDecode(batch: ByteBuffer): Long {
        repeat(WARMUP) { Protocol.decode(batch, collect) }
        val before = threads.currentThreadAllocatedBytes
        repeat(MEASURED) { Protocol.decode(batch, collect) }
        val after = threads.currentThreadAllocatedBytes
        return (after - before) / MEASURED
    }

    /**
     * The arena is read in place. A decoder that copied the buffer, or scanned past the
     * records it was given, would cost more for the same record in a larger arena.
     */
    @Test
    fun pr4_reading_a_record_does_not_cost_what_the_arena_around_it_costs() {
        val short = bytesPerDecode(textChange("a keystroke's worth", trailing = 0))
        val long = bytesPerDecode(textChange("a keystroke's worth", trailing = 64 * 1024))
        println("pr4 one text change: ${short}B in a small arena, ${long}B in a 64KB arena")
        assertEquals(
            PropertyKind.Text,
            (sink as Mutation.SetProp).property,
            "the measurement decoded something other than the text change",
        )
        assertTrue(
            long <= short + 16,
            "the same record cost ${long}B out of a 64KB arena and ${short}B out of a small " +
                "one. The arena is the Host's memory and is meant to be read where it lies; " +
                "a cost that follows its size means something copied it.",
        )
    }

    /**
     * How much of a text change's cost is the text. One copy of the text means the cost
     * grows by about one byte per character; each further copy adds the same again, and a
     * copy that goes through `char` adds two.
     */
    @Test
    fun pr4_a_text_change_costs_one_pass_over_the_text() {
        val text = "x".repeat(SHORT_TEXT)
        val longer = "x".repeat(SHORT_TEXT + EXTRA_TEXT)
        val small = bytesPerDecode(textChange(text))
        val large = bytesPerDecode(textChange(longer))
        val perCharacter = (large - small).toDouble() / EXTRA_TEXT
        println(
            "pr4 text change: ${small}B for $SHORT_TEXT characters, ${large}B for " +
                "${SHORT_TEXT + EXTRA_TEXT}, ${"%.2f".format(perCharacter)}B per character",
        )
        assertEquals(
            PropertyValue.Text(longer),
            (sink as Mutation.SetProp).value,
            "the measurement decoded something other than the text change",
        )
        assertTrue(
            perCharacter <= BYTES_PER_CHARACTER_CEILING,
            "one ASCII character of changed text costs " +
                "${"%.2f".format(perCharacter)} bytes on this side, and the ceiling is " +
                "$BYTES_PER_CHARACTER_CEILING. One byte is the string Compose is handed; " +
                "anything above that was copied on the way to it.",
        )
    }

    private companion object {
        const val ENVELOPE_BYTES = 12
        const val SET_PROP_BYTES = 24
        const val WARMUP = 20_000
        const val MEASURED = 20_000
        const val SHORT_TEXT = 16
        const val EXTRA_TEXT = 4_000

        /**
         * Bytes per character of changed text.
         *
         * One byte is the string itself, and one byte is what this should cost. Three is
         * what it costs today: the decoder reads the arena through a `CharsetDecoder`,
         * which builds the text as `char` (two bytes each) before the `String` compacts it
         * back down to one. It does that because it has to report invalid UTF-8 rather
         * than replace it silently, which `String(bytes, UTF_8)` will not do.
         *
         * So this ceiling is not the target. It is here to stop a fourth copy arriving
         * unnoticed while the third one is still there.
         */
        const val BYTES_PER_CHARACTER_CEILING = 3.2
    }
}
