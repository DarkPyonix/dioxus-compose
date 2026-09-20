package org.thisisthepy.dioxus.compose.renderer

import androidx.compose.ui.test.ExperimentalTestApi
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.runComposeUiTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import org.thisisthepy.dioxus.compose.protocol.HostEvent
import org.thisisthepy.dioxus.compose.protocol.Mutation
import org.thisisthepy.dioxus.compose.protocol.PropertyKind
import org.thisisthepy.dioxus.compose.protocol.PropertyValue
import org.thisisthepy.dioxus.compose.protocol.WidgetKind

private const val FIELD = 1
private const val CHANGE_HANDLER = 21L

private fun field(vararg extra: Mutation) = listOf(
    Mutation.Create(FIELD, WidgetKind.TextField),
    Mutation.SetProp(FIELD, PropertyKind.OnValueChange, PropertyValue.Integer(CHANGE_HANDLER)),
    *extra,
)

@OptIn(ExperimentalTestApi::class)
class TextFieldTest {
    @Test
    fun fr5_editing_value_stays_in_the_renderer_and_notifies_the_host() = runComposeUiTest {
        val connection = FakeHostConnection(field())
        setContent { DioxusContent(rememberDioxusHost(connection)) }

        onNodeWithTag(nodeTestTag(FIELD)).performTextInput("hi")
        waitForIdle()
        // The debounce window has to elapse before the Host hears about the edit.
        mainClock.advanceTimeBy(TEXT_CHANGED_DEBOUNCE_MILLIS * 2)
        waitForIdle()

        onNodeWithTag(nodeTestTag(FIELD)).assertTextEquals("hi")
        val changes = connection.events.filterIsInstance<HostEvent.TextChanged>()
        assertEquals("hi", changes.lastOrNull()?.text, "events were ${connection.events}")
    }

    @Test
    fun fr5_host_set_text_replaces_the_value_when_no_composition_is_active() = runComposeUiTest {
        val connection = FakeHostConnection(field())
        connection.respondWith {
            HostResponse(listOf(Mutation.SetText(FIELD, "from host", -1, -1)))
        }
        lateinit var host: DioxusHost
        setContent { host = rememberDioxusHost(connection) ; DioxusContent(host) }

        host.dispatch(HostEvent.Clicked(FIELD, 0))
        waitForIdle()

        onNodeWithTag(nodeTestTag(FIELD)).assertTextEquals("from host")
    }

    @Test
    fun fr5_enter_during_composition_only_commits_the_composition() {
        // An Enter press while the IME is composing must never reach the Host: it means
        // "commit the composition". Driving a real IME is a manual check (SPEC §6); this
        // pins the decision the key handler makes.
        assertFalse(
            shouldSubmitOnEnter(
                composing = true,
                multiline = false,
                shiftPressed = false,
                hasSubmitHandler = true,
            ),
        )
        assertTrue(
            shouldSubmitOnEnter(
                composing = false,
                multiline = false,
                shiftPressed = false,
                hasSubmitHandler = true,
            ),
        )
    }

    @Test
    fun fr5_shift_enter_in_a_multiline_field_is_a_newline_not_a_submit() {
        assertFalse(
            shouldSubmitOnEnter(
                composing = false,
                multiline = true,
                shiftPressed = true,
                hasSubmitHandler = true,
            ),
        )
        assertTrue(
            shouldSubmitOnEnter(
                composing = false,
                multiline = true,
                shiftPressed = false,
                hasSubmitHandler = true,
            ),
        )
    }
}
