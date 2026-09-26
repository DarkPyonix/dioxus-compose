@file:OptIn(androidx.compose.ui.InternalComposeUiApi::class)

package dioxus.compose.ui.platform

import androidx.compose.ui.input.key.Key

/**
 * The Compose key a platform key number means.
 *
 * A table because the two numberings have nothing to do with each other: the platform
 * numbers keys by where they sit on the board, and Compose names them by what they are.
 * Only the keys that have a meaning of their own are here. A key that types a character
 * carries that character in the event beside it, and a screen reading text wants the
 * character rather than the position.
 *
 * Unknown is a real answer. A key nobody mapped still reaches the scene with its
 * character, so typing works before every key in the world has a line here.
 */
internal fun composeKey(platformKey: Int): Key = when (platformKey) {
    0x24 -> Key.Enter
    0x30 -> Key.Tab
    0x31 -> Key.Spacebar
    0x33 -> Key.Backspace
    0x35 -> Key.Escape
    0x75 -> Key.Delete
    0x7B -> Key.DirectionLeft
    0x7C -> Key.DirectionRight
    0x7D -> Key.DirectionDown
    0x7E -> Key.DirectionUp
    0x73 -> Key.MoveHome
    0x77 -> Key.MoveEnd
    0x74 -> Key.PageUp
    0x79 -> Key.PageDown
    else -> Key.Unknown
}
