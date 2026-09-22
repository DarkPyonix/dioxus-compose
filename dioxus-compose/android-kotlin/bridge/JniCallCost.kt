package dioxus.compose.ui.platform

import android.os.Build
import android.util.Log

/**
 * What one boundary call costs on this device.
 *
 * The published figures are about 115 ns for an ordinary JNI call and about 35 ns for one
 * that skips the thread state transition. Both are measured here against an empty shim, so
 * the number is the transition and nothing else, and the result is logged with the device
 * it came from.
 *
 * Run it on the UI thread, which is where real boundary calls happen.
 */
object JniCallCost {
    private const val TAG = "dxc-jni-cost"
    private const val WARMUP = 200_000
    private const val SAMPLES = 2_000_000

    fun measureAndLog() {
        val plain = measure(WARMUP, SAMPLES) { nativeNoop() }
        val fast = measure(WARMUP, SAMPLES) { nativeNoopFast() }
        Log.i(TAG, "device=${Build.MODEL} api=${Build.VERSION.SDK_INT}")
        Log.i(TAG, "jni plain: ${format(plain)} per call over $SAMPLES calls")
        Log.i(TAG, "jni without the state transition: ${format(fast)} per call over $SAMPLES calls")
    }

    private inline fun measure(warmup: Int, samples: Int, call: () -> Int): Double {
        var sink = 0
        for (index in 0 until warmup) sink += call()
        val started = System.nanoTime()
        for (index in 0 until samples) sink += call()
        val elapsed = System.nanoTime() - started
        // Keeps the loop from being optimised away without costing a branch per call.
        if (sink == Int.MIN_VALUE) Log.v(TAG, "unreachable")
        return elapsed.toDouble() / samples
    }

    private fun format(nanos: Double) = String.format("%.2f ns", nanos)
}
