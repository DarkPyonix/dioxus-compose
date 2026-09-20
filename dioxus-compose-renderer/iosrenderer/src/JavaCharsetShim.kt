@file:Suppress("PackageDirectoryMismatch")

package java.nio.charset

import java.nio.ByteBuffer

/**
 * The slice of `java.nio.charset` the generated protocol codec uses. See `JavaNioShim.kt`.
 *
 * The codec decodes strings strictly: malformed UTF-8 is an error, never a replacement
 * character, because a bad string means the two sides disagree about the arena and the
 * Renderer has to report `ProtocolError` rather than draw nonsense (SPEC NFR-7).
 */
class CharacterCodingException : Exception("input is not valid UTF-8")

enum class CodingErrorAction { REPORT }

class CharsetDecoder internal constructor() {
    fun onMalformedInput(action: CodingErrorAction): CharsetDecoder = this

    fun onUnmappableCharacter(action: CodingErrorAction): CharsetDecoder = this

    /** Always strict; `REPORT` is the only action this shim offers. */
    fun decode(buffer: ByteBuffer): String =
        try {
            buffer.toByteArray().decodeToString(throwOnInvalidSequence = true)
        } catch (_: kotlin.text.CharacterCodingException) {
            throw CharacterCodingException()
        }
}

class Charset internal constructor(val name: String) {
    fun newDecoder(): CharsetDecoder = CharsetDecoder()
}

object StandardCharsets {
    val UTF_8 = Charset("UTF-8")
}
