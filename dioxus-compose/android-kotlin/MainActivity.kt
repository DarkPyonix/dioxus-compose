package dioxus.compose

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.ComposeView
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.ui.platform.DioxusRuntime
import dioxus.compose.ui.platform.JniCallCost

/**
 * The Android host.
 *
 * Kotlin owns the process and the frame loop: the Activity puts a `ComposeView` on screen,
 * the interpreter draws the Host's tree inside it, and `DioxusContent` drives the frame
 * calls from Compose's own frame clock. Rust runs as a cdylib inside this process and
 * never starts a loop of its own.
 *
 * The Host outlives the Activity, so a configuration change recomposes and nothing else.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val host = DioxusRuntime.host()
        val view = ComposeView(this)
        view.setContent {
            DioxusContent(host, Modifier.fillMaxSize().safeDrawingPadding())
        }
        setContentView(view)
        if (intent?.getBooleanExtra(EXTRA_MEASURE_CALL_COST, false) == true) {
            JniCallCost.measureAndLog()
        }
    }

    override fun onStart() {
        super.onStart()
        DioxusRuntime.start()
    }

    override fun onStop() {
        DioxusRuntime.stop()
        super.onStop()
    }

    private companion object {
        /**
         * Starting with `--ez dxc.measureCallCost true` measures the per-call cost of the
         * boundary before anything else runs, and writes it to the log.
         */
        const val EXTRA_MEASURE_CALL_COST = "dxc.measureCallCost"
    }
}
