@file:OptIn(
    androidx.compose.ui.InternalComposeUiApi::class,
    androidx.compose.ui.ExperimentalComposeUiApi::class,
)

package dioxus.compose.ui.platform

import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.platform.PlatformContext
import androidx.compose.ui.platform.PlatformTextInputMethodRequest
import androidx.compose.ui.platform.WindowInfo
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.CommitTextCommand
import androidx.compose.ui.text.input.SetComposingTextCommand
import androidx.compose.ui.unit.IntSize
import kotlinx.coroutines.awaitCancellation

// What a scene needs from the thing hosting it, answered without a toolkit.
//
// Almost all of it has a default that is already right. Two answers are required, and one
// more matters: a field asks to be typed into through `startInputMethod`, and until
// something answers that, a field can be focused and clicked and stay empty however much
// is typed at it. Key events are not how text arrives in Compose; an input session is.

/**
 * The window a scene is in, as far as Compose needs to know.
 *
 * Focus is reported as held, because a window that never says so leaves a text field
 * drawing no caret: the field is told the window it sits in is not the one being typed at.
 * Following the real focus is the next thing this learns, and the shell already hears it.
 */
class NativeWindowInfo(private val size: () -> IntSize) : WindowInfo {
    override val isWindowFocused: Boolean get() = true
    override val containerSize: IntSize get() = size()
}

/** Pointer or keyboard, which decides whether focus is drawn. */
class NativeInputModeManager : InputModeManager {
    override val inputMode: InputMode get() = InputMode.Keyboard

    override fun requestInputMode(inputMode: InputMode): Boolean =
        inputMode == InputMode.Keyboard || inputMode == InputMode.Touch
}

/**
 * The session a focused field opens to be typed into.
 *
 * Held for as long as the field is focused. `startInputMethod` is suspended for the life
 * of the session and returns when it is cancelled, which is Compose's way of saying the
 * field has gone away; what is kept here is torn down at that moment so that a later
 * keystroke is not delivered into a field nobody is looking at.
 */
class NativeTextInput {

    private var session: PlatformTextInputMethodRequest? = null

    /** True where some field is waiting to be typed into. */
    val isActive: Boolean get() = session != null

    suspend fun run(request: PlatformTextInputMethodRequest): Nothing {
        session = request
        try {
            awaitCancellation()
        } finally {
            session = null
        }
    }

    /**
     * Puts text into the focused field, finished.
     *
     * What a key that types a letter produces, and what an input method produces once the
     * reader has chosen. The caret goes after it, which is what the one means.
     */
    fun commit(text: String) {
        val request = session ?: return
        request.onEditCommand(listOf(CommitTextCommand(AnnotatedString(text), 1)))
    }

    /**
     * Shows text the reader is still composing.
     *
     * The part of typing Korean, Japanese and Chinese that a character-at-a-time path
     * cannot do: the letters under composition are in the field, marked, and are replaced
     * as the reader goes rather than accumulating. An empty string ends the composition
     * without committing anything, which is what an input method asks for when the reader
     * backs out of it.
     */
    fun compose(text: String) {
        val request = session ?: return
        request.onEditCommand(listOf(SetComposingTextCommand(AnnotatedString(text), 1)))
    }
}

/**
 * The platform, as the scene sees it.
 *
 * Everything not overridden here has a default that suits a window we own, and the
 * defaults are the reason this is short rather than the reason it is unfinished.
 */
class NativePlatformContext(
    private val size: () -> IntSize,
    private val textInput: NativeTextInput,
) : PlatformContext {

    override val windowInfo: WindowInfo = NativeWindowInfo(size)
    override val inputModeManager: InputModeManager = NativeInputModeManager()

    override suspend fun startInputMethod(request: PlatformTextInputMethodRequest): Nothing =
        textInput.run(request)
}
