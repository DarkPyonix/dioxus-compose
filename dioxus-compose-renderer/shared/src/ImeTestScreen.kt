package org.thisisthepy.dioxus.compose

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeContentPadding
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp

/**
 * Manual check for the IME acceptance checklist in docs/SPEC.md §6.
 *
 * This is stock Compose, deliberately: it establishes what the platform text stack does in a
 * native-image build before the interpreter sits between it and the Host. If Korean does not
 * compose here, nothing above it can fix that (INTENT D4).
 *
 * Run it as a native binary with `native/scripts/build-native.sh` and `native/scripts/smoke-test.sh`.
 */
@Composable
fun ImeTestScreen() {
    MaterialTheme {
        Column(
            modifier = Modifier.fillMaxSize().safeContentPadding().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("IME 확인 (SPEC §6)", style = MaterialTheme.typography.titleMedium)
            Text(
                "1) '안녕하세요' 입력  2) 조합 중 백스페이스  3) 조합 중 화살표 이동  " +
                    "4) 문장 중간에 한글 삽입  5) 멀티라인에서 조합 중 Enter  " +
                    "6) 한글 섞인 긴 텍스트 붙여넣기  7) 일본어/중국어 후보창 위치",
                style = MaterialTheme.typography.bodySmall,
            )

            var singleLine by remember { mutableStateOf(TextFieldValue()) }
            OutlinedTextField(
                value = singleLine,
                onValueChange = { singleLine = it },
                label = { Text("한 줄 입력") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { singleLine = TextFieldValue() }),
                modifier = Modifier.fillMaxWidth(),
            )
            FieldReadout(singleLine)

            var multiLine by remember { mutableStateOf(TextFieldValue()) }
            OutlinedTextField(
                value = multiLine,
                onValueChange = { multiLine = it },
                label = { Text("여러 줄 입력") },
                modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp),
            )
            FieldReadout(multiLine)
        }
    }
}

/**
 * Shows what the field actually holds. Selection and code points make the failures visible:
 * a composition that resets shows up as a jumped cursor or as separated jamo.
 */
@Composable
private fun FieldReadout(value: TextFieldValue) {
    val selection: TextRange = value.selection
    val codePoints = value.text.map { it.code.toString(16) }.joinToString(" ")
    Text(
        "길이 ${value.text.length} · 커서 ${selection.start}..${selection.end} · U+ $codePoints",
        style = MaterialTheme.typography.bodySmall,
    )
}
