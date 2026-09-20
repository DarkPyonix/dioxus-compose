package org.thisisthepy.dioxus.compose

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.material3.Text

/**
 * Placeholder for the Android host (SPEC PR-5, M6).
 *
 * The real one drives the renderer through the generated JNI shim, the way `desktop`
 * drives it through the C boundary. This module exists now only to keep the Android
 * product configuration and the manifest, which are tedious to recreate.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent { Text("dioxus-compose Android host: not implemented yet (M6)") }
    }
}
