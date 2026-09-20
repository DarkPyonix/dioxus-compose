# Desktop accessibility experiment (SPEC NFR-8, §7)

Does the AWT accessibility bridge survive the native-image build?

The question §7 asks is not whether Compose supports accessibility. It is whether the
native image keeps the path Compose already has. Those two failures look identical from
the outside, so every check here is run twice, once on the JVM dev shell and once on the
native image, and the difference between them is the answer.

## How the bridge works

1. Compose attaches the window's `Accessible` to the platform. On macOS
   `androidx.compose.ui.platform.a11y.Accessibility_desktopKt.initializeAccessible` calls
   `org.jetbrains.skiko.AccessibilityKt.initializeCAccessible`, which is a native method in
   `libskiko-macos-<arch>.dylib`.
2. That native method looks up `sun.lwawt.macosx.CAccessible.getCAccessible` through JNI.
3. From then on AppKit drives the tree from Objective-C. For every element it calls back
   into `sun.lwawt.macosx.CAccessibility` to ask for the children, the role, the name and
   the value.

Step 3 is the part the closed-world analysis cannot see, and it is where the image broke.

## Tools

`ax-dump.swift` walks a running process's accessibility tree with
`AXUIElementCreateApplication` and prints roles, labels and values. This is the same tree
VoiceOver reads, so it is real evidence rather than a proxy, and it can run unattended.

    swift ax-dump.swift <pid> [maxDepth]

It needs the calling terminal to hold the Accessibility permission (System Settings >
Privacy & Security > Accessibility). Without it the reads fail with `-25211`
(`kAXErrorAPIDisabled`) and the tool says so, rather than printing an empty tree and
letting an unrelated permission problem look like a renderer failure.

What it cannot tell you is whether VoiceOver's *speech* is sensible: reading order, whether
a label is useful rather than merely present, and whether focus follows the caret. That
part needs a human, and the procedure is at the end of this file.

## Automated procedure

Run both halves and compare. The trees will not match element for element, because the JVM
run and the smoke test compose different screens, but the shape must match: a window with a
labelled `AXStaticText` and `AXButton` under it, not a bare window.

JVM dev shell:

    cd dioxus-compose-renderer
    ./kotlin run -m native &
    # find the app process, not the Compose Hot Reload dev tools process
    pgrep -fl jbrsdk
    swift ../experiments/accessibility/ax-dump.swift <pid>

Native image:

    cd dioxus-compose-renderer
    ./native/scripts/build-native.sh
    ./native/scripts/smoke-test.sh &
    swift ../experiments/accessibility/ax-dump.swift "$(pgrep -f smoke_host)"

A native-image run that aborts with `Abort trap: 6` and an
`NSInvalidArgumentException ... object cannot be nil` the moment the dumper touches it is
the regression this experiment exists to catch. The abort happens inside AppKit while it
builds the children array, and there is no Java stack trace, so the smoke test's exit code
(134) is the signal to watch.

## Result, 2026-09-20, macOS arm64, Liberica NIK 25 Full

| | JVM dev shell | native image |
|---|---|---|
| elements in tree | 14 | 1 |
| labelled controls | `AXStaticText`, `AXTextField`, `AXButton` | none |
| process after the dump | alive | aborted, exit 134 |

The JVM tree is complete and labelled:

    AXWindow subrole=AXStandardWindow title=DioxusCompose
      AXUnknown
        AXGroup
          AXUnknown
            AXStaticText desc=clicks: 0 value=clicks: 0
            AXTextField desc=type here
            AXButton desc=increment

So Compose Desktop supports accessibility and the screen carries the semantics. The native
image exposes the window and nothing under it, and aborts the moment anything asks for the
window's children. Under §7's required split this is a native-image configuration gap, not
a Substrate or Compose limitation. **NFR-8 does not hold on the native image today.**

### Verdict on the fix: one gap closed, not yet working

`native/src/AccessibilityReachabilityFeature.kt` closes one proven gap. It does not make
accessibility work. The tree is still one element and the abort is unchanged.

What was proven. Building with `--exact-reachability-metadata` and
`-R:MissingRegistrationReportingMode=Warn` makes the image report what it would have
refused, and it named the gap exactly:

    MissingResourceRegistrationError: Cannot access resource bundle with name
      'com.sun.accessibility.internal.resources.accessibility'
      java.util.ResourceBundle.getBundle(ResourceBundle.java:921)
      javax.accessibility.AccessibleBundle.toDisplayString(AccessibleBundle.java:92)
      javax.accessibility.AccessibleBundle.toString(AccessibleBundle.java:126)
      sun.lwawt.macosx.CAccessibility.getAccessibleRole(CAccessibility.java:943)
      sun.lwawt.macosx.CAccessibility._addChildren(CAccessibility.java:1038)
      sun.lwawt.macosx.CAccessibility.getChildrenAndRolesImpl(CAccessibility.java:703)

Registering the bundle removes that error: it no longer appears in the diagnostic run, and
the role display strings (`push button`, `check box`) are now in the binary.

What is still unknown. With the bundle registered, the diagnostic run reports **no**
missing registration on the accessibility path at all, and the process still aborts with
the identical stack: AppKit's `childrenOfParent` inserting nil into the children array.
Something on that path still returns null to Objective-C, and the closed-world analysis is
no longer the thing complaining, so the remaining cause is not a metadata gap that GraalVM
can see.

The leading remaining suspect, not confirmed. Objective-C reads the role string as the
`key` field of `javax.accessibility.AccessibleRole` through
`GetFieldID(AccessibleRole, "key")`, but `key` is declared on the superclass
`AccessibleBundle`. Registering the superclass chain for JNI as well was tried and did
**not** fix it, so either that is not the cause or the registration is not doing what the
lookup needs. The next step is to confirm from the Objective-C side which value is null:
build a debug image, attach lldb, and break on `-[CommonComponentAccessibility
createWithParent:accessible:role:index:withEnv:withView:]` to see whether it is the role
string or the returned child that comes back nil. That distinguishes a JNI field lookup
failure from a role the ObjC role table does not map.

### What was already fine

The Objective-C half and the `Java_sun_lwawt_macosx_CAccessib*` entry points were never
missing. `build-native.sh` force-loads the whole `libawt_lwawt.a`, which carries all 14 of
them, so the link needed no change. Checking the built `.dylib` with `nm` is not a useful
test here: it is stripped, and the IME natives that are known to work show up as absent
too. Check the archive instead:

    nm -g "$GRAALVM_HOME/lib/static/darwin-aarch64/libawt_lwawt.a" \
      | grep -c "T _Java_sun_lwawt_macosx_CAccessib"    # 14

### Note on the smoke test

`smoke-test.sh` composes a Column, a Text and a Button, not two text fields. It exits 0 on
its own, because nothing asks the process for its accessibility tree. It exits 134 only
when the dumper or VoiceOver attaches. A green smoke test is therefore not evidence for
NFR-8, and the accessibility check has to be run as its own step.

## Manual VoiceOver procedure (needs a human)

§7's first checkbox cannot be automated. The tree dump proves the elements and labels are
exposed; only a listener can confirm VoiceOver speaks them usefully. Run this on the
native image, then repeat on the JVM run and record any difference.

Run it on the JVM today. On the native image the procedure stops at step 2: turning
VoiceOver on is enough to make the process abort, for the reason above. That is itself the
expected result for now, and step 2 is the check that tells you the remaining bug is fixed.
Keep the whole procedure written down so that the day it gets past step 2 there is
something to run rather than something to invent.

Setup. Grant Accessibility permission to the terminal you will use, or the dump step
fails with `-25211`. Build and start the app:

    cd dioxus-compose-renderer
    ./native/scripts/build-native.sh
    ./native/scripts/smoke-test.sh

Turn VoiceOver on with Command-F5. Keep the VoiceOver caption panel visible (VoiceOver
Utility > General > Show caption panel) so the spoken text can be transcribed and pasted
into the result rather than recalled.

Steps, recording the spoken text verbatim for each:

1. Click the app window. VoiceOver should announce the window by name.
2. Press Control-Option-Shift-Down to enter the window's contents. Expected: it does not
   announce an empty group.
3. Press Control-Option-Right repeatedly to walk every element. Expected: the static text
   is read with its content, and the button is read with its label followed by "button".
   Record the order and compare it with the visual order.
4. On the button, press Control-Option-Space. Expected: the button activates and the label
   text updates, and VoiceOver announces the new value.
5. Press Tab, then Shift-Tab. Expected: focus moves between the focusable controls, the
   focus ring is visible, and VoiceOver announces each newly focused element. This is §7's
   Tab traversal checkbox.
6. If the screen under test has a text field, focus it and type. Expected: the typed
   characters are echoed. Then switch to a Korean input source and type a syllable.
   Expected: the composing text is announced without the process aborting. This crosses the
   input method path and the accessibility path at once, which is where the two Features
   could interfere.
7. Turn VoiceOver off with Command-F5 and confirm the app is still running. A crash that
   only appears on VoiceOver teardown is a distinct failure worth recording.

Record for each step: spoken text, whether it matched the expectation, and whether the JVM
run differed. A step that differs between the two is a native-image configuration gap. A
step that fails identically on both is a Compose or a semantics gap in the screen, and
belongs upstream or in the widget layer rather than here.

Windows Narrator is the same shape of check and is not covered by this experiment; the
renderer has no Windows build yet.
