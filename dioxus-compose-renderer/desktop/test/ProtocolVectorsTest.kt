package dioxus.compose.test

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.runComposeUiTest
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.Modifier as ProtocolModifier
import dioxus.compose.protocol.Mutation
import dioxus.compose.protocol.Paint
import dioxus.compose.protocol.ShapeRole
import dioxus.compose.protocol.SpaceRole
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.Protocol
import dioxus.compose.protocol.WidgetKind
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.DioxusHost
import dioxus.compose.runtime.rememberDioxusHost
import dioxus.compose.tooling.FakeHostConnection
import dioxus.compose.ui.node.nodeTestTag

/**
 * Keeps the interpreter in lockstep with the Rust Host: the checked-in vectors are the bytes
 * both sides are tested against.
 *
 * The reference batch deliberately addresses nodes it never creates, so it doubles as the
 * crash-isolation check: every bad record becomes a `ProtocolError` event rather than
 * taking the process down.
 */
@OptIn(ExperimentalTestApi::class)
class ProtocolVectorsTest {
    @Test
    fun pr4_checked_in_mutation_vector_is_interpreted_without_crashing() = runComposeUiTest {
        val mutations = decodeVector("mutations.bin")
        assertTrue(mutations.isNotEmpty(), "the vector must contain records")

        val connection = FakeHostConnection(mutations)
        lateinit var host: DioxusHost
        setContent { host = rememberDioxusHost(connection) ; DioxusContent(host) }
        waitForIdle()

        val root = host.table.node(1)
        assertEquals(WidgetKind.Box, root?.widget)
        onNodeWithTag(nodeTestTag(1)).assertIsDisplayed()

        val errors = connection.events.filterIsInstance<HostEvent.ProtocolError>()
        val badRecords = mutations.count { mutation ->
            when (mutation) {
                is Mutation.SetProp -> mutation.nodeId != 1
                is Mutation.SetModifier -> mutation.nodeId != 1
                is Mutation.Insert -> mutation.nodeId != 1
                is Mutation.Move -> mutation.nodeId != 1
                is Mutation.Remove -> mutation.nodeId != 1
                is Mutation.SetText -> mutation.nodeId != 1
                // The theme applies to the tree, not to a node, so it is never a bad record.
                is Mutation.SetTheme -> false
                is Mutation.AppendText -> mutation.nodeId != 1
                is Mutation.Create -> false
            }
        }
        assertEquals(badRecords, errors.size, "every unknown-node record must be reported")
    }

    @Test
    fun fr10_every_modifier_variant_in_the_vector_round_trips() {
        val modifiers = decodeVector("mutations.bin")
            .filterIsInstance<Mutation.SetModifier>()
            .map { it.modifier }
        assertEquals(
            listOf(
                ProtocolModifier.Empty,
                ProtocolModifier.Padding(16f),
                ProtocolModifier.FillMaxWidth,
                ProtocolModifier.FillMaxHeight,
                ProtocolModifier.Width(120f),
                ProtocolModifier.Height(48f),
                ProtocolModifier.Size(20f, 30f),
                ProtocolModifier.Background(Paint.Literal(0xFF112233.toInt())),
                ProtocolModifier.Clickable(42L),
                ProtocolModifier.Background(Paint.Role(ColorRole.Surface)),
                ProtocolModifier.PaddingRole(SpaceRole.Md),
                ProtocolModifier.PaddingEach(1f, 2f, 3f, 4f),
                ProtocolModifier.Weight(0.5f),
                ProtocolModifier.Shape(4f, 8f, 12f, 16f),
                ProtocolModifier.ShapeRole(ShapeRole.Large),
                ProtocolModifier.Border(2f, Paint.Role(ColorRole.Outline)),
                ProtocolModifier.Elevation(6f),
            ),
            modifiers,
        )
    }

    private fun decodeVector(name: String): List<Mutation> {
        val bytes = vectorFile(name).readBytes()
        val mutations = mutableListOf<Mutation>()
        Protocol.decode(ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN), mutations::add)
        return mutations
    }

    private fun vectorFile(name: String): File {
        var directory: File? = File(System.getProperty("user.dir")).absoluteFile
        while (directory != null) {
            val candidate = File(directory, "dioxus-compose/tests/vectors/$name")
            if (candidate.isFile) return candidate
            directory = directory.parentFile
        }
        error("protocol vector $name not found above ${System.getProperty("user.dir")}")
    }
}
