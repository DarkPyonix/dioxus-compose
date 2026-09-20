package dioxus.compose.foundation

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isShiftPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import dioxus.compose.protocol.ColorRole
import dioxus.compose.protocol.HostEvent
import dioxus.compose.protocol.PropertyKind
import dioxus.compose.design.LocalDesignTheme
import dioxus.compose.runtime.EventDispatcher
import dioxus.compose.ui.node.Node
import dioxus.compose.ui.textStyle
import dioxus.compose.ui.node.HostText

/** Quiet period before a `TextChanged` notification is sent (SPEC FR-5, "debounced"). */
const val TEXT_CHANGED_DEBOUNCE_MILLIS: Long = 120

/**
 * The uncontrolled TextField (SPEC FR-5, INTENT D5).
 *
 * The editing value and the IME composition live in `remember` and never round-trip through
 * the Host. The Host learns about edits through debounced `TextChanged` and through the
 * commit events `TextSubmitted` and `FocusLost`; it changes the value only with `SetText`,
 * which is held back while a composition is in progress.
 *
 * A Host `on_key_down` handler is offered the key first and its result decides consumption
 * (SPEC FR-12); `TextSubmitted` remains for a field that only declares `on_submit`.
 *
 * SPEC-GAP: FR-5 says `TextChanged` is debounced but names no interval; 120 ms is chosen
 * here and must be confirmed in the SPEC.
 *
 * Key events are never dispatched while composing: Enter during a Korean composition means
 * "commit the composition", not "submit".
 */
@Composable
internal fun HostTextField(node: Node, modifier: Modifier, dispatcher: EventDispatcher) {
    val nodeId = node.id
    var value by remember(nodeId) {
        mutableStateOf(TextFieldValue(node.text(PropertyKind.Text)))
    }
    var appliedHostRevision by remember(nodeId) { mutableStateOf(0L) }
    var focused by remember(nodeId) { mutableStateOf(false) }

    val multiline = node.flag(PropertyKind.Multiline, default = false)
    val enabled = node.flag(PropertyKind.Enabled, default = true)
    val changeHandler = node.handler(PropertyKind.OnValueChange)
    val submitHandler = node.handler(PropertyKind.OnSubmit)
    val keyDownHandler = node.handler(PropertyKind.OnKeyDown)
    val focusLostHandler = node.handler(PropertyKind.OnFocusLost)
    val placeholder = node.text(PropertyKind.Placeholder)
    val hostText = node.hostText
    val composing = value.composition != null
    // FR-14.4: the field takes its type role and its colours from the design system, the
    // same way a Text does.
    val theme = LocalDesignTheme.current
    val textStyle = node.textStyle(theme)

    // A Host SetText is applied only once the composition has finished (SPEC FR-5).
    LaunchedEffect(hostText, composing) {
        if (hostText != null && hostText.revision != appliedHostRevision && !composing) {
            value = TextFieldValue(hostText.text, selection = hostText.selection(hostText.text))
            appliedHostRevision = hostText.revision
        }
    }

    // Debounced change notification. The Host is told what the field now shows; it never
    // sends the value back, so the composition cannot be reset by the round trip.
    if (changeHandler != null) {
        LaunchedEffect(nodeId, changeHandler) {
            var lastSent = value.text
            snapshotFlow { value.text }.collectLatest { text ->
                if (text == lastSent) return@collectLatest
                delay(TEXT_CHANGED_DEBOUNCE_MILLIS)
                lastSent = text
                dispatcher.dispatch(HostEvent.TextChanged(nodeId, changeHandler, text))
            }
        }
    }

    val inputModifier = modifier
        .onFocusChanged { state ->
            if (focused && !state.isFocused && focusLostHandler != null) {
                dispatcher.dispatch(HostEvent.FocusLost(nodeId, focusLostHandler))
            }
            focused = state.isFocused
        }
        // Preview is required because this intercepts Enter before the editor inserts a
        // newline; the Host's synchronous result decides whether it is consumed (SPEC FR-12).
        //
        // `value.composition` is read here rather than from the `composing` snapshot above so
        // the state is the one that exists at the moment the key arrives: that read is the
        // guard that keeps Enter away from Rust during a Korean composition (SPEC §6).
        .onPreviewKeyEvent { event ->
            val composingNow = value.composition != null
            val consumedByHandler = keyDownHandler != null &&
                dispatchKeyDown(nodeId, keyDownHandler, event, composingNow, dispatcher)
            if (consumedByHandler) {
                true
            } else if (!isDispatchableKeyDown(event, composingNow)) {
                false
            } else if (!shouldSubmitOnEnter(
                    composing = composingNow,
                    multiline = multiline,
                    shiftPressed = event.isShiftPressed,
                    hasSubmitHandler = submitHandler != null,
                )
            ) {
                false
            } else {
                dispatcher.dispatch(HostEvent.TextSubmitted(nodeId, submitHandler!!, value.text))
            }
        }

    BasicTextField(
        value = value,
        onValueChange = { value = it },
        modifier = inputModifier,
        enabled = enabled,
        singleLine = !multiline,
        textStyle = textStyle,
        cursorBrush = SolidColor(theme.color(ColorRole.Primary)),
        decorationBox = { inner ->
            Box {
                if (value.text.isEmpty() && placeholder.isNotEmpty()) {
                    BasicText(
                        placeholder,
                        style = textStyle.copy(color = theme.color(ColorRole.OnSurfaceVariant)),
                    )
                }
                inner()
            }
        },
    )
}

private fun HostText.selection(text: String): TextRange {
    // u32::MAX in both fields means "no selection given"; the caret goes to the end.
    if (selectionStart == -1 && selectionEnd == -1) return TextRange(text.length)
    val start = selectionStart.coerceIn(0, text.length)
    val end = selectionEnd.coerceIn(0, text.length)
    return TextRange(start, end)
}

/**
 * Whether an Enter key press becomes a `TextSubmitted` event (SPEC FR-5, FR-12).
 *
 * While an IME composition is in progress Enter means "commit the composition", so nothing
 * is dispatched and the key is left to the editor. In a multiline field Shift+Enter is a
 * newline, never a submit.
 */
internal fun shouldSubmitOnEnter(
    composing: Boolean,
    multiline: Boolean,
    shiftPressed: Boolean,
    hasSubmitHandler: Boolean,
): Boolean = when {
    composing -> false
    !hasSubmitHandler -> false
    multiline && shiftPressed -> false
    else -> true
}
