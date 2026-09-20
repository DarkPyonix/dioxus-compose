@file:Suppress("PackageDirectoryMismatch")

package java.nio

import kotlinx.cinterop.ByteVar
import kotlinx.cinterop.CPointer
import kotlinx.cinterop.get
import kotlinx.cinterop.set

/**
 * The slice of `java.nio` that the generated protocol codec uses (SPEC PR-4).
 *
 * `Protocol.gen.kt` is generated from the Rust schema by `cargo run -p dioxus-compose --bin
 * codegen` and is written against `java.nio.ByteBuffer`. Kotlin/Native has no JDK, so this
 * file supplies the same names and the same semantics for the operations the codec performs.
 * Compiling the generated file unchanged is the point: the iOS renderer must decode exactly
 * what the desktop renderer decodes, and a second hand-written decoder would drift.
 *
 * This is a stopgap. The proper fix is for codegen to emit a buffer-neutral codec, which is a
 * change to `dioxus-compose/src/codegen.rs` on the Rust side.
 *
 * Two backings, both read in place (PR-4 forbids copying the batch):
 * - a Kotlin `ByteArray`, for the event buffer the renderer fills and hands to the Host;
 * - a raw `CPointer`, for the Host's arena, which this side only reads.
 *
 * Both byte orders are honoured even though the protocol is little-endian (PR-4): the codec
 * reads the buffer's previous order, sets its own, and restores the old one when it is done,
 * and a JDK buffer starts out big-endian.
 */
class ByteOrder private constructor(private val label: String) {
    override fun toString(): String = label

    companion object {
        val LITTLE_ENDIAN = ByteOrder("LITTLE_ENDIAN")
        val BIG_ENDIAN = ByteOrder("BIG_ENDIAN")
    }
}

class ByteBuffer private constructor(
    private val array: ByteArray?,
    private val pointer: CPointer<ByteVar>?,
    private val offset: Int,
    private val capacity: Int,
) {
    private var position: Int = 0
    private var limit: Int = capacity
    private var order: ByteOrder = ByteOrder.BIG_ENDIAN

    companion object {
        fun wrap(array: ByteArray): ByteBuffer = ByteBuffer(array, null, 0, array.size)

        /** Wraps `length` bytes of foreign memory. The caller keeps ownership. */
        fun wrapPointer(pointer: CPointer<ByteVar>, length: Int): ByteBuffer =
            ByteBuffer(null, pointer, 0, length)
    }

    fun order(): ByteOrder = order

    fun order(value: ByteOrder): ByteBuffer {
        order = value
        return this
    }

    fun position(): Int = position

    fun position(value: Int): ByteBuffer {
        require(value in 0..limit) { "position $value out of bounds [0, $limit]" }
        position = value
        return this
    }

    fun limit(): Int = limit

    fun limit(value: Int): ByteBuffer {
        require(value in 0..capacity) { "limit $value out of bounds [0, $capacity]" }
        limit = value
        if (position > limit) position = limit
        return this
    }

    fun remaining(): Int = limit - position

    fun clear(): ByteBuffer {
        position = 0
        limit = capacity
        return this
    }

    /** Independent position and limit over the same bytes; no copy. */
    fun duplicate(): ByteBuffer =
        ByteBuffer(array, pointer, offset, capacity).also {
            it.position = position
            it.limit = limit
            it.order = order
        }

    /** The bytes from the current position to the limit, as a buffer of its own; no copy. */
    fun slice(): ByteBuffer =
        ByteBuffer(array, pointer, offset + position, limit - position).also { it.order = order }

    private fun byteAt(index: Int): Byte {
        require(index in 0 until capacity) { "index $index out of bounds [0, $capacity)" }
        val absolute = offset + index
        return array?.get(absolute) ?: pointer!![absolute]
    }

    private fun setByteAt(index: Int, value: Byte) {
        require(index in 0 until capacity) { "index $index out of bounds [0, $capacity)" }
        val absolute = offset + index
        if (array != null) array[absolute] = value else pointer!![absolute] = value
    }

    fun get(index: Int): Byte = byteAt(index)

    private fun readBits(index: Int, width: Int): Long {
        var value = 0L
        for (step in 0 until width) {
            val byte = if (order === ByteOrder.LITTLE_ENDIAN) width - 1 - step else step
            value = (value shl 8) or (byteAt(index + byte).toLong() and 0xff)
        }
        return value
    }

    private fun writeBits(value: Long, width: Int) {
        requireSpace(width)
        for (step in 0 until width) {
            val byte = if (order === ByteOrder.LITTLE_ENDIAN) step else width - 1 - step
            setByteAt(position + byte, ((value ushr (8 * step)) and 0xff).toByte())
        }
        position += width
    }

    fun getShort(index: Int): Short = readBits(index, 2).toShort()

    fun getInt(index: Int): Int = readBits(index, 4).toInt()

    fun getLong(index: Int): Long = readBits(index, 8)

    fun put(value: Byte): ByteBuffer {
        requireSpace(1)
        setByteAt(position, value)
        position += 1
        return this
    }

    fun put(values: ByteArray): ByteBuffer {
        requireSpace(values.size)
        for (index in values.indices) {
            setByteAt(position + index, values[index])
        }
        position += values.size
        return this
    }

    fun putShort(value: Short): ByteBuffer {
        writeBits(value.toLong() and 0xffffL, 2)
        return this
    }

    fun putInt(value: Int): ByteBuffer {
        writeBits(value.toLong() and 0xffff_ffffL, 4)
        return this
    }

    fun putLong(value: Long): ByteBuffer {
        writeBits(value, 8)
        return this
    }

    private fun requireSpace(bytes: Int) {
        if (remaining() < bytes) {
            throw IndexOutOfBoundsException("buffer overflow: $bytes bytes into ${remaining()}")
        }
    }

    /** Copies the bytes between position and limit out. Used only to decode strings. */
    internal fun toByteArray(): ByteArray =
        ByteArray(remaining()) { byteAt(position + it) }
}
